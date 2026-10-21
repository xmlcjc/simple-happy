//! B 站直播接口适配。
//!
//! 只使用**免签名**的公开接口:
//! - 分区: `xlive/web-interface/v1/index/getWebAreaList`
//! - 房间列表: `room/v1/area/getRoomList`(免 WBI 签名; `second/getList` 会返回 -352)
//! - 房间信息: `room/v1/Room/get_info` + `live_user/v1/UserInfo/get_anchor_in_room`
//! - 取流: `xlive/web-room/v2/index/getRoomPlayInfo`
//!
//! 所有请求都要带 `Referer: https://live.bilibili.com/`, 否则 CDN 会 403。

use serde_json::Value;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use super::models::{
    LiveCategory, LiveCategoryGroup, LiveRoomDetail, LiveRoomSummary, LiveStream, LiveStreamInfo,
};

pub const PLATFORM_ID: &str = "bilibili";
pub const PLATFORM_NAME: &str = "哔哩哔哩";

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const REFERER: &str = "https://live.bilibili.com/";
const PAGE_SIZE: u32 = 30;

const URL_AREAS: &str = "https://api.live.bilibili.com/xlive/web-interface/v1/index/getWebAreaList?source_id=2";

pub struct Bilibili {
    http: HttpClient,
    /// `buvid3/buvid4` cookie。搜索接口裸请求会 412, 必须先取一次指纹再带上。
    buvid: std::sync::Mutex<Option<String>>,
}

impl Bilibili {
    pub fn new(http: HttpClient) -> Self {
        Self {
            http,
            buvid: std::sync::Mutex::new(None),
        }
    }

    pub async fn categories(&self) -> Result<Vec<LiveCategoryGroup>, AppError> {
        let value = self.get_json(URL_AREAS).await?;
        Ok(parse_categories(&value))
    }

    pub async fn rooms(
        &self,
        parent_area_id: &str,
        area_id: &str,
        page: u32,
    ) -> Result<Vec<LiveRoomSummary>, AppError> {
        let parent = normalize_id(parent_area_id, "一级分区")?;
        let area = normalize_id(area_id, "二级分区").unwrap_or_else(|_| "0".to_string());
        let url = format!(
            "https://api.live.bilibili.com/room/v1/area/getRoomList?platform=web&parent_area_id={parent}&area_id={area}&sort_type=online&page={}&page_size={PAGE_SIZE}",
            page.max(1)
        );
        let value = self.get_json(&url).await?;
        Ok(parse_rooms(&value))
    }

    pub async fn room_detail(&self, room_id: &str) -> Result<LiveRoomDetail, AppError> {
        let id = normalize_id(room_id, "房间号")?;
        let info_url = format!("https://api.live.bilibili.com/room/v1/Room/get_info?room_id={id}");
        let anchor_url = format!(
            "https://api.live.bilibili.com/live_user/v1/UserInfo/get_anchor_in_room?roomid={id}"
        );
        let (info, anchor) = tokio::try_join!(self.get_json(&info_url), self.get_json(&anchor_url))?;
        parse_room_detail(&info, &anchor)
            .ok_or_else(|| AppError::NotFound(format!("直播间 {id} 不存在")))
    }

    pub async fn stream_urls(&self, room_id: &str) -> Result<LiveStreamInfo, AppError> {
        let id = normalize_id(room_id, "房间号")?;
        let url = format!(
            "https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo?room_id={id}&protocol=0,1&format=0,1,2&codec=0,1&qn=10000&platform=web&ptype=8"
        );
        let value = self.get_json(&url).await?;
        Ok(LiveStreamInfo {
            room_id: id,
            is_live: value.pointer("/data/live_status").and_then(Value::as_i64) == Some(1),
            streams: parse_play_streams(&value),
        })
    }

    async fn get_json(&self, url: &str) -> Result<Value, AppError> {
        let response = self
            .http
            .client()
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, REFERER)
            .send()
            .await?
            .error_for_status()?;
        let value: Value = response.json().await?;
        match value.get("code").and_then(Value::as_i64).unwrap_or(0) {
            0 => Ok(value),
            code => Err(AppError::BadRequest(format!(
                "B站直播接口返回错误 {code}: {}",
                text(&value, "message")
            ))),
        }
    }

    /// 搜索直播间。`buvid3/4` 只在首次调用时取, 之后复用(过期由站点重新下发)。
    pub async fn search(&self, keyword: &str, page: u32) -> Result<Vec<LiveRoomSummary>, AppError> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(AppError::BadRequest("搜索关键词不能为空".to_string()));
        }
        let cookie = self.buvid_cookie().await?;
        let url = format!(
            "https://api.bilibili.com/x/web-interface/search/type?context=&search_type=live&cover_type=user_cover&keyword={}&page={}",
            urlencoding::encode(keyword),
            page.max(1)
        );
        let response = self
            .http
            .client()
            .get(&url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, "https://search.bilibili.com/")
            .header(reqwest::header::COOKIE, cookie)
            .send()
            .await?
            .error_for_status()?;
        let value: Value = response.json().await?;
        match value.get("code").and_then(Value::as_i64).unwrap_or(0) {
            0 => Ok(parse_search(&value)),
            code => Err(AppError::BadRequest(format!(
                "B站搜索失败 {code}: {}",
                text(&value, "message")
            ))),
        }
    }

    async fn buvid_cookie(&self) -> Result<String, AppError> {
        if let Some(cookie) = self
            .buvid
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
        {
            return Ok(cookie);
        }
        let value = self
            .get_json("https://api.bilibili.com/x/frontend/finger/spi")
            .await?;
        let b3 = value
            .pointer("/data/b_3")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let b4 = value
            .pointer("/data/b_4")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if b3.is_empty() {
            return Err(AppError::BadRequest("B站未返回 buvid".to_string()));
        }
        let cookie = format!("buvid3={b3}; buvid4={b4}");
        *self.buvid.lock().unwrap_or_else(|err| err.into_inner()) = Some(cookie.clone());
        Ok(cookie)
    }
}

pub fn parse_search(value: &Value) -> Vec<LiveRoomSummary> {
    let Some(items) = value
        .pointer("/data/result/live_room")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let room_id = text(item, "roomid");
            if room_id.is_empty() || room_id == "0" {
                return None;
            }
            // 搜索结果的标题/分区名都带 `<em class="keyword">` 高亮标签, 统一剔除。
            let title = strip_highlight(&text(item, "title"));
            let mut cover = text(item, "user_cover");
            if cover.is_empty() {
                cover = text(item, "cover");
            }
            if let Some(rest) = cover.strip_prefix("//") {
                cover = format!("https://{rest}");
            }
            Some(LiveRoomSummary {
                platform: PLATFORM_ID.to_string(),
                room_id,
                title,
                cover,
                user_name: text(item, "uname"),
                online: number(item, "online"),
                area_name: strip_highlight(&text(item, "cate_name")),
            })
        })
        .collect()
}

fn strip_highlight(raw: &str) -> String {
    regex::Regex::new(r"</?em[^>]*>")
        .map(|re| re.replace_all(raw, "").to_string())
        .unwrap_or_else(|_| raw.to_string())
}

/// 图床常给 `http://` 或协议相对地址; WebView 里必须统一成 https。
fn upgrade_https(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("//") {
        return format!("https://{rest}");
    }
    if let Some(rest) = raw.strip_prefix("http://") {
        return format!("https://{rest}");
    }
    raw.to_string()
}

/// 房间号/分区号只允许数字, 既避免拼进 URL 的参数被注入, 也顺手拦掉空值。
fn normalize_id(raw: &str, label: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::BadRequest(format!("{label}无效: {raw}")));
    }
    Ok(trimmed.to_string())
}

fn text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn number(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|item| {
            item.as_u64()
                .or_else(|| item.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0)
}

pub fn parse_categories(value: &Value) -> Vec<LiveCategoryGroup> {
    let Some(groups) = value.pointer("/data/data").and_then(Value::as_array) else {
        return Vec::new();
    };
    groups
        .iter()
        .filter_map(|group| {
            let id = text(group, "id");
            if id.is_empty() {
                return None;
            }
            let children = group
                .get("list")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|child| {
                            let id = text(child, "id");
                            if id.is_empty() {
                                return None;
                            }
                            Some(LiveCategory {
                                id,
                                name: text(child, "name"),
                                icon: upgrade_https(&text(child, "pic")),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(LiveCategoryGroup { id, name: text(group, "name"), children })
        })
        .collect()
}

pub fn parse_rooms(value: &Value) -> Vec<LiveRoomSummary> {
    let Some(rooms) = value.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    rooms
        .iter()
        .filter_map(|room| {
            let room_id = text(room, "roomid");
            if room_id.is_empty() || room_id == "0" {
                return None;
            }
            // 列表里的 cover 可能是空串, 依次回退到用户封面。
            let cover = ["cover", "user_cover", "system_cover"]
                .iter()
                .map(|key| text(room, key))
                .find(|value| !value.is_empty())
                .unwrap_or_default();
            Some(LiveRoomSummary {
                platform: PLATFORM_ID.to_string(),
                room_id,
                title: text(room, "title"),
                cover,
                user_name: text(room, "uname"),
                online: number(room, "online"),
                area_name: text(room, "area_name"),
            })
        })
        .collect()
}

pub fn parse_room_detail(info: &Value, anchor: &Value) -> Option<LiveRoomDetail> {
    let room = info.get("data")?;
    let room_id = text(room, "room_id");
    if room_id.is_empty() || room_id == "0" {
        return None;
    }
    let anchor_info = anchor.pointer("/data/info").cloned().unwrap_or(Value::Null);
    let cover = ["user_cover", "keyframe", "background"]
        .iter()
        .map(|key| text(room, key))
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    Some(LiveRoomDetail {
        platform: PLATFORM_ID.to_string(),
        room_id,
        title: text(room, "title"),
        cover,
        user_name: text(&anchor_info, "uname"),
        avatar: text(&anchor_info, "face"),
        online: number(room, "online"),
        followers: number(room, "attention"),
        description: text(room, "description"),
        area_name: text(room, "area_name"),
        parent_area_name: text(room, "parent_area_name"),
        is_live: room.get("live_status").and_then(Value::as_i64) == Some(1),
    })
}

/// 从 `getRoomPlayInfo` 的响应里拼出可播放地址。
///
/// 顺序按「浏览器可直接播」优先: hls(ts) → hls(fmp4) → flv。
/// flv 地址会校验 `Referer`, 而 Referer 是浏览器禁止脚本设置的请求头, 因此在
/// WebView 里只有 hls 可用; flv 仍一并返回, 留给以后可能的外部播放器。
pub fn parse_play_streams(value: &Value) -> Vec<LiveStream> {
    let Some(playurl) = value.pointer("/data/playurl_info/playurl") else {
        return Vec::new();
    };
    let qualities = quality_names(playurl);
    let Some(streams) = playurl.get("stream").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut out: Vec<(u8, LiveStream)> = Vec::new();
    for stream in streams {
        let protocol = text(stream, "protocol_name");
        let Some(formats) = stream.get("format").and_then(Value::as_array) else {
            continue;
        };
        for format in formats {
            let format_name = text(format, "format_name");
            let (kind, rank) = match (protocol.as_str(), format_name.as_str()) {
                ("http_hls", "ts") => ("hls", 0u8),
                ("http_hls", "fmp4") => ("hls", 1u8),
                ("http_stream", "flv") => ("flv", 2u8),
                _ => continue,
            };
            let Some(codecs) = format.get("codec").and_then(Value::as_array) else {
                continue;
            };
            for codec in codecs {
                if text(codec, "codec_name") != "avc" {
                    continue;
                }
                let Some(url) = build_stream_url(codec) else {
                    continue;
                };
                let qn = codec.get("current_qn").and_then(Value::as_i64).unwrap_or(0);
                let quality = qualities
                    .iter()
                    .find(|(value, _)| *value == qn)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_else(|| "默认".to_string());
                out.push((rank, LiveStream { format: kind.to_string(), quality, url }));
            }
        }
    }
    // 同一清晰度可能有多种封装, 先按可播优先级排序再去重, 才能留下 ts 而不是 fmp4。
    out.sort_by_key(|(rank, _)| *rank);
    let mut deduped: Vec<LiveStream> = Vec::new();
    for (_, stream) in out {
        if deduped
            .iter()
            .any(|item| item.format == stream.format && item.quality == stream.quality)
        {
            continue;
        }
        deduped.push(stream);
    }
    deduped
}

fn build_stream_url(codec: &Value) -> Option<String> {
    let base = codec.get("base_url").and_then(Value::as_str)?;
    let host = codec
        .pointer("/url_info/0/host")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let extra = codec
        .pointer("/url_info/0/extra")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if base.is_empty() {
        return None;
    }
    Some(format!("{host}{base}{extra}"))
}

fn quality_names(playurl: &Value) -> Vec<(i64, String)> {
    playurl
        .get("g_qn_desc")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let qn = item.get("qn").and_then(Value::as_i64)?;
                    Some((qn, text(item, "desc")))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_results_and_strips_highlight_tags() {
        let value = serde_json::json!({
            "code": 0,
            "data": { "result": { "live_room": [
                { "roomid": 24530995, "title": "削弱前的最后一舞",
                  "cate_name": "<em class=\"keyword\">英雄联盟</em>",
                  "cover": "//i0.hdslb.com/a.jpg", "uname": "主播", "online": "109957" },
                { "roomid": 0, "title": "占位" }
            ]}}
        });
        let rooms = parse_search(&value);
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "24530995");
        assert_eq!(rooms[0].area_name, "英雄联盟");
        assert_eq!(rooms[0].cover, "https://i0.hdslb.com/a.jpg");
        assert_eq!(rooms[0].online, 109957);
    }

    #[test]
    fn normalizes_numeric_ids_and_rejects_others() {
        assert_eq!(normalize_id(" 23335083 ", "房间号").unwrap(), "23335083");
        assert!(normalize_id("", "房间号").is_err());
        assert!(normalize_id("1&page=2", "房间号").is_err());
    }

    #[test]
    fn parses_parent_and_child_categories() {
        let value = serde_json::json!({
            "code": 0,
            "data": { "data": [
                { "id": 2, "name": "网游", "list": [
                    { "id": "86", "name": "英雄联盟" },
                    { "id": "", "name": "坏数据" }
                ]},
                { "id": 0, "name": "无效", "list": [] }
            ]}
        });
        let groups = parse_categories(&value);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "2");
        assert_eq!(groups[0].children.len(), 1);
        assert_eq!(groups[0].children[0].id, "86");
        assert_eq!(groups[0].children[0].name, "英雄联盟");
    }

    #[test]
    fn parses_room_list_and_skips_placeholder_rooms() {
        let value = serde_json::json!({
            "code": 0,
            "data": [
                { "roomid": 1917445033, "title": "标题", "uname": "主播", "online": 306274,
                  "cover": "", "user_cover": "https://i0.hdslb.com/a.jpg", "area_name": "团播" },
                { "roomid": 0, "title": "占位" }
            ]
        });
        let rooms = parse_rooms(&value);
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "1917445033");
        assert_eq!(rooms[0].cover, "https://i0.hdslb.com/a.jpg");
        assert_eq!(rooms[0].online, 306274);
        assert_eq!(rooms[0].user_name, "主播");
    }

    #[test]
    fn merges_room_info_with_anchor_info() {
        let info = serde_json::json!({
            "code": 0,
            "data": { "room_id": 23335083, "title": "标题", "online": 31412, "attention": 27717,
                      "live_status": 1, "description": "简介", "area_name": "颜值",
                      "parent_area_name": "娱乐", "user_cover": "https://i0.hdslb.com/c.jpg" }
        });
        let anchor = serde_json::json!({
            "code": 0,
            "data": { "info": { "uname": "主播", "face": "https://i2.hdslb.com/f.jpg" } }
        });
        let detail = parse_room_detail(&info, &anchor).expect("should parse");
        assert_eq!(detail.room_id, "23335083");
        assert_eq!(detail.user_name, "主播");
        assert_eq!(detail.avatar, "https://i2.hdslb.com/f.jpg");
        assert_eq!(detail.followers, 27717);
        assert!(detail.is_live);
        assert_eq!(detail.parent_area_name, "娱乐");
    }

    #[test]
    fn missing_room_payload_is_not_found_instead_of_empty_card() {
        assert!(parse_room_detail(&serde_json::json!({ "code": 0, "data": {} }), &Value::Null).is_none());
    }

    /// hls 必须排在 flv 前面: WebView 里 flv 因 Referer 限制不可播。
    #[test]
    fn orders_hls_before_flv_and_marks_quality() {
        let value = serde_json::json!({
            "code": 0,
            "data": {
                "live_status": 1,
                "playurl_info": { "playurl": {
                    "g_qn_desc": [{ "qn": 10000, "desc": "原画" }, { "qn": 400, "desc": "蓝光" }],
                    "stream": [
                        { "protocol_name": "http_stream", "format": [
                            { "format_name": "flv", "codec": [
                                { "codec_name": "avc", "current_qn": 10000,
                                  "base_url": "/live/bvc/1.flv?x=1",
                                  "url_info": [{ "host": "https://cn.bilivideo.com", "extra": "&y=2" }] }
                            ]}
                        ]},
                        { "protocol_name": "http_hls", "format": [
                            { "format_name": "fmp4", "codec": [
                                { "codec_name": "avc", "current_qn": 10000,
                                  "base_url": "/live/bvc/1/index.m4s", "url_info": [{ "host": "https://d1.bilivideo.com" }] }
                            ]},
                            { "format_name": "ts", "codec": [
                                { "codec_name": "hevc", "current_qn": 10000,
                                  "base_url": "/live/bvc/hevc.m3u8", "url_info": [{ "host": "https://d1.bilivideo.com" }] },
                                { "codec_name": "avc", "current_qn": 10000,
                                  "base_url": "/live/bvc/1.m3u8", "url_info": [{ "host": "https://d1.bilivideo.com" }] }
                            ]}
                        ]}
                    ]
                }}
            }
        });
        let info = LiveStreamInfo {
            room_id: "1".to_string(),
            is_live: true,
            streams: parse_play_streams(&value),
        };
        assert_eq!(info.streams.len(), 2);
        assert_eq!(info.streams[0].format, "hls");
        assert_eq!(info.streams[0].url, "https://d1.bilivideo.com/live/bvc/1.m3u8");
        assert_eq!(info.streams[0].quality, "原画");
        assert_eq!(info.streams[1].format, "flv");
        assert_eq!(info.streams[1].url, "https://cn.bilivideo.com/live/bvc/1.flv?x=1&y=2");
    }

    #[test]
    fn offline_room_has_no_streams() {
        let value = serde_json::json!({ "code": 0, "data": { "live_status": 0, "playurl_info": null } });
        assert!(parse_play_streams(&value).is_empty());
    }
}
