//! 虎牙直播适配。
//!
//! 全部使用公开 Web 接口, 无需登录/签名:
//! - 分区: `live.cdn.huya.com/liveconfig/game/bussLive`
//! - 房间列表: `www.huya.com/cache.php?m=LiveList&do=getLiveListByPage`
//! - 房间详情 + 取流: `mp.huya.com/cache.php?m=Live&do=profileRoom`
//!
//! **播放一律走 FLV**: 虎牙的 HLS 地址(`*.hls.huya.com`)对 Web 端已恒定 403
//! (各种 UA/Referer 实测全部被拒), 而 `baseSteamInfoList` 里的 FLV 地址可用,
//! 且 CDN 回了 `Access-Control-Allow-Origin: *`, 浏览器里可以直接交给 flv.js
//! (simple_live / dart_simple_live 也是这么做的)。同一条线路的 FLV 请求会随机
//! 返回 403(约一半), 所以这里给出全部线路, 由前端在连接失败时立刻换下一条。

use serde_json::Value;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use super::models::{
    LiveCategory, LiveCategoryGroup, LiveQuality, LiveRoomDetail, LiveRoomSummary, LiveStream,
    LiveStreamInfo,
};

pub const PLATFORM_ID: &str = "huya";
pub const PLATFORM_NAME: &str = "虎牙直播";

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

const GROUP_IDS: [(&str, &str); 4] = [("1", "网游"), ("2", "单机"), ("3", "手游"), ("8", "娱乐")];

pub struct Huya {
    http: HttpClient,
}

impl Huya {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn categories(&self) -> Vec<LiveCategoryGroup> {
        let mut groups = Vec::new();
        for (id, name) in GROUP_IDS {
            let url = format!("https://live.cdn.huya.com/liveconfig/game/bussLive?bussType={id}");
            let children = match self.get_json(&url, "https://www.huya.com/").await {
                Ok(value) => parse_sub_categories(&value),
                Err(_) => Vec::new(),
            };
            groups.push(LiveCategoryGroup {
                id: id.to_string(),
                name: name.to_string(),
                children,
            });
        }
        groups
    }

    pub async fn rooms(&self, area_id: &str, page: u32) -> Result<Vec<LiveRoomSummary>, AppError> {
        let gid = normalize_id(area_id, "分区")?;
        let url = format!(
            "https://www.huya.com/cache.php?m=LiveList&do=getLiveListByPage&tagAll=0&gameId={gid}&page={}",
            page.max(1)
        );
        let value = self.get_json(&url, "https://www.huya.com/").await?;
        Ok(parse_rooms(&value))
    }

    pub async fn search(&self, keyword: &str, page: u32) -> Result<Vec<LiveRoomSummary>, AppError> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(AppError::BadRequest("搜索关键词不能为空".to_string()));
        }
        let start = (page.max(1) - 1) * 20;
        let url = format!(
            "https://search.cdn.huya.com/?m=Search&do=getSearchContent&q={}&uid=0&v=4&typ=-5&livestate=0&rows=20&start={start}",
            urlencoding::encode(keyword)
        );
        let value = self.get_json(&url, "https://www.huya.com/").await?;
        Ok(parse_search(&value))
    }

    pub async fn room_detail(&self, room_id: &str) -> Result<LiveRoomDetail, AppError> {
        let id = normalize_id(room_id, "房间号")?;
        let value = self.profile(&id).await?;
        parse_room_detail(&value, &id)
            .ok_or_else(|| AppError::NotFound(format!("直播间 {id} 不存在")))
    }

    /// 取流。`qn` 为选中的码率(iBitRate), 为空时取接口给的最高档。
    pub async fn stream_urls(
        &self,
        room_id: &str,
        qn: Option<i64>,
    ) -> Result<LiveStreamInfo, AppError> {
        let id = normalize_id(room_id, "房间号")?;
        let value = self.profile(&id).await?;
        let is_live = value.pointer("/data/liveStatus").and_then(Value::as_str) == Some("ON");
        // 清晰度列表来自 `stream.hls.rateArray`(蓝光8M/蓝光4M/超清/流畅),
        // 与 simple_live 一致, 取流时用 `ratio=<码率>` 选档。
        let qualities = parse_qualities(&value);
        let mut streams = Vec::new();
        if is_live {
            let target = qn
                .and_then(|rate| qualities.iter().find(|item| item.qn == rate).cloned())
                .or_else(|| qualities.first().cloned());
            let bit_rate = target.as_ref().map(|quality| quality.qn).unwrap_or(0);
            let quality_name = target
                .as_ref()
                .map(|quality| quality.name.clone())
                .unwrap_or_else(|| "原画".to_string());
            // 全部线路都给出去: 同一线路会随机 403, 只给一条会让前端反复重连。
            for url in flv_urls(&value, bit_rate) {
                streams.push(LiveStream {
                    format: "flv".to_string(),
                    quality: quality_name.clone(),
                    url,
                });
            }
        }
        Ok(LiveStreamInfo {
            room_id: id,
            is_live,
            streams,
            qualities,
        })
    }

    async fn profile(&self, room_id: &str) -> Result<Value, AppError> {
        let url =
            format!("https://mp.huya.com/cache.php?m=Live&do=profileRoom&roomid={room_id}");
        self.get_json(&url, &format!("https://www.huya.com/{room_id}"))
            .await
    }

    async fn get_json(&self, url: &str, referer: &str) -> Result<Value, AppError> {
        let response = self
            .http
            .client()
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, referer)
            .send()
            .await?
            .error_for_status()?;
        let value: Value = response.json().await?;
        match value.get("status").and_then(Value::as_i64).unwrap_or(200) {
            200 => Ok(value),
            code => Err(AppError::BadRequest(format!(
                "虎牙直播接口返回错误 {code}: {}",
                text(&value, "message")
            ))),
        }
    }
}

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

pub fn parse_sub_categories(value: &Value) -> Vec<LiveCategory> {
    let Some(items) = value.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            // `gid` 在不同接口版本里是对象 / 数字 / 字符串, 统一取第一个数字。
            let raw = match item.get("gid") {
                Some(Value::Object(map)) => map.get("value").map(|v| text_value(v)).unwrap_or_default(),
                Some(Value::Number(n)) => n.to_string(),
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            let id = raw.split(',').next().unwrap_or_default().trim().to_string();
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            Some(LiveCategory {
                id: id.clone(),
                name: text(item, "gameFullName"),
                // 虎牙不直接给图标, 但其游戏图有固定 CDN 规则(与 simple_live 一致)。
                icon: format!("https://huyaimg.msstatic.com/cdnimage/game/{id}-MS.jpg"),
            })
        })
        .collect()
}

fn text_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

pub fn parse_rooms(value: &Value) -> Vec<LiveRoomSummary> {
    let Some(items) = value.pointer("/data/datas").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let room_id = text(item, "profileRoom");
            if room_id.is_empty() || room_id == "0" {
                return None;
            }
            let mut title = text(item, "introduction");
            if title.is_empty() {
                title = text(item, "roomName");
            }
            let mut cover = text(item, "screenshot");
            // 万恶的 `http://` 图床在 https 页面会被拦; 统一升级为 https。
            if let Some(rest) = cover.strip_prefix("http://") {
                cover = format!("https://{rest}");
            }
            Some(LiveRoomSummary {
                platform: PLATFORM_ID.to_string(),
                room_id,
                title,
                cover,
                user_name: text(item, "nick"),
                online: number(item, "totalCount"),
                area_name: text(item, "gameFullName"),
            })
        })
        .collect()
}

/// 搜索结果: `response["3"]["docs"]` 是直播间列表(虎牙 ES 接口的分组编号)。
pub fn parse_search(value: &Value) -> Vec<LiveRoomSummary> {
    let Some(items) = value
        .pointer("/response/3/docs")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let room_id = text(item, "room_id");
            if room_id.is_empty() || room_id == "0" {
                return None;
            }
            let mut title = text(item, "game_introduction");
            if title.is_empty() {
                title = text(item, "game_roomName");
            }
            let mut cover = text(item, "game_screenshot");
            if let Some(rest) = cover.strip_prefix("http://") {
                cover = format!("https://{rest}");
            }
            Some(LiveRoomSummary {
                platform: PLATFORM_ID.to_string(),
                room_id,
                title,
                cover,
                user_name: text(item, "game_nick"),
                online: number(item, "game_total_count"),
                area_name: text(item, "game_name"),
            })
        })
        .collect()
}

pub fn parse_room_detail(value: &Value, room_id: &str) -> Option<LiveRoomDetail> {
    let data = value.get("data")?;
    let live = data.get("liveData").cloned().unwrap_or(Value::Null);
    let profile = data.get("profileInfo").cloned().unwrap_or(Value::Null);
    let mut title = text(&live, "roomName");
    if title.is_empty() {
        title = text(&live, "introduction");
    }
    let mut cover = text(&live, "screenshot");
    if let Some(rest) = cover.strip_prefix("http://") {
        cover = format!("https://{rest}");
    }
    Some(LiveRoomDetail {
        platform: PLATFORM_ID.to_string(),
        room_id: room_id.to_string(),
        title,
        cover,
        user_name: text(&profile, "nick"),
        avatar: text(&profile, "avatar180"),
        online: number(&live, "totalCount"),
        followers: 0,
        description: text(&live, "contentIntro"),
        area_name: text(&live, "gameFullName"),
        parent_area_name: String::new(),
        is_live: data.get("liveStatus").and_then(Value::as_str) == Some("ON"),
        // 虎牙公开接口未提供开播时间。
        live_start_time: 0,
    })
}

/// 清晰度列表: `stream.hls.rateArray`(回退到 flv 的同名字段)。
/// 与 simple_live 一致: 跳过 HDR 档, 顺序保持接口下发的高→低。
pub fn parse_qualities(value: &Value) -> Vec<LiveQuality> {
    let Some(list) = value
        .pointer("/data/stream/hls/rateArray")
        .or_else(|| value.pointer("/data/stream/flv/rateArray"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| {
            let name = text(item, "sDisplayName");
            let rate = item.get("iBitRate").and_then(|raw| {
                raw.as_i64()
                    .or_else(|| raw.as_str().and_then(|text| text.parse().ok()))
            })?;
            // `iBitRate=0` 是虎牙的"超清/默认"档 —— 此时地址里自带的 ratio 已可用,
            // 不能把它当成无效档位丢掉(否则默认只能播最低码率)。
            if name.is_empty() || rate < 0 || name.contains("HDR") {
                return None;
            }
            Some(LiveQuality { qn: rate, name })
        })
        .collect()
}

/// FLV 播放线路, 按 `iWebPriorityRate` 从高到低排序。
///
/// 地址形如 `{sFlvUrl}/{sStreamName}.{sFlvUrlSuffix}?{sFlvAntiCode}`; 指定码率时
/// 追加 `ratio=<码率>`。`sFlvAntiCode` 已含 `wsSecret/wsTime/fm/ctype/fs/t`,
/// 直接可用, 不必自己算签名。
pub fn flv_urls(value: &Value, bit_rate: i64) -> Vec<String> {
    let Some(lines) = value
        .pointer("/data/stream/baseSteamInfoList")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut entries: Vec<(u64, String)> = Vec::new();
    for item in lines {
        let host = text(item, "sFlvUrl");
        let name = text(item, "sStreamName");
        let anti = text(item, "sFlvAntiCode");
        if host.is_empty() || name.is_empty() {
            continue;
        }
        let suffix = match text(item, "sFlvUrlSuffix") {
            empty if empty.is_empty() => "flv".to_string(),
            value => value,
        };
        let mut url = format!("{host}/{name}.{suffix}");
        let mut query = anti;
        if bit_rate > 0 {
            query = if query.is_empty() {
                format!("ratio={bit_rate}")
            } else {
                format!("{query}&ratio={bit_rate}")
            };
        }
        if !query.is_empty() {
            url.push('?');
            url.push_str(&query);
        }
        // 同一地址可能被接口重复下发(不同码率的同名线路)。
        if entries.iter().any(|(_, existing)| existing == &url) {
            continue;
        }
        entries.push((number(item, "iWebPriorityRate"), url));
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.into_iter().map(|(_, url)| url).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sub_categories_with_mixed_gid_shapes() {
        let value = serde_json::json!({
            "status": 200,
            "data": [
                { "gid": { "value": "1,2" }, "gameFullName": "英雄联盟" },
                { "gid": 2, "gameFullName": "星际争霸" },
                { "gid": "", "gameFullName": "坏数据" }
            ]
        });
        let subs = parse_sub_categories(&value);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].id, "1");
        assert_eq!(subs[1].id, "2");
    }

    #[test]
    fn parses_room_list_and_upgrades_cover_scheme() {
        let value = serde_json::json!({
            "status": 200,
            "data": { "datas": [
                { "profileRoom": 518518, "introduction": "海南", "roomName": "",
                  "screenshot": "http://live-cover.msstatic.com/a.jpg",
                  "nick": "主播", "totalCount": "3423877", "gameFullName": "英雄联盟" },
                { "profileRoom": "0", "introduction": "占位" }
            ]}
        });
        let rooms = parse_rooms(&value);
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "518518");
        assert_eq!(rooms[0].title, "海南");
        assert_eq!(rooms[0].cover, "https://live-cover.msstatic.com/a.jpg");
        assert_eq!(rooms[0].online, 3423877);
    }

    #[test]
    fn parses_search_results() {
        let value = serde_json::json!({
            "response": { "3": { "docs": [
                { "room_id": 518518, "game_introduction": "海南", "game_roomName": "",
                  "game_screenshot": "http://live-cover.msstatic.com/a.jpg",
                  "game_nick": "主播", "game_total_count": 3423877, "game_name": "英雄联盟" }
            ]}}
        });
        let rooms = parse_search(&value);
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "518518");
        assert_eq!(rooms[0].cover, "https://live-cover.msstatic.com/a.jpg");
        assert_eq!(rooms[0].online, 3423877);
    }

    #[test]
    fn parses_room_detail_from_profile_info() {
        let value = serde_json::json!({
            "status": 200,
            "data": {
                "liveStatus": "ON",
                "liveData": { "roomName": "", "introduction": "海南", "contentIntro": "职业选手",
                              "totalCount": 3423877, "gameFullName": "英雄联盟",
                              "screenshot": "http://live-cover.msstatic.com/a.jpg" },
                "profileInfo": { "nick": "Letme", "avatar180": "https://huyaimg.msstatic.com/a.jpg" }
            }
        });
        let detail = parse_room_detail(&value, "518518").expect("should parse");
        assert_eq!(detail.title, "海南");
        assert_eq!(detail.user_name, "Letme");
        assert!(detail.is_live);
        assert_eq!(detail.cover, "https://live-cover.msstatic.com/a.jpg");
    }

    #[test]
    fn builds_flv_urls_with_priority_and_ratio() {
        let value = serde_json::json!({
            "data": { "stream": { "baseSteamInfoList": [
                { "sFlvUrl": "http://al.flv.huya.com/src", "sStreamName": "a-b-c",
                  "sFlvUrlSuffix": "flv", "sFlvAntiCode": "wsSecret=1&wsTime=2",
                  "iWebPriorityRate": -1 },
                { "sFlvUrl": "http://hs.flv.huya.com/src", "sStreamName": "a-b-c",
                  "sFlvUrlSuffix": "flv", "sFlvAntiCode": "wsSecret=1&wsTime=2",
                  "iWebPriorityRate": 50 },
                // 重复线路应被去重
                { "sFlvUrl": "http://hs.flv.huya.com/src", "sStreamName": "a-b-c",
                  "sFlvUrlSuffix": "flv", "sFlvAntiCode": "wsSecret=1&wsTime=2",
                  "iWebPriorityRate": 50 }
            ]}}
        });
        let urls = flv_urls(&value, 8000);
        assert_eq!(urls.len(), 2);
        // 优先级高的 HS 在前, 且都追加了目标码率。
        assert_eq!(
            urls[0],
            "http://hs.flv.huya.com/src/a-b-c.flv?wsSecret=1&wsTime=2&ratio=8000"
        );
        assert_eq!(
            urls[1],
            "http://al.flv.huya.com/src/a-b-c.flv?wsSecret=1&wsTime=2&ratio=8000"
        );
    }

    #[test]
    fn flv_urls_defaults_suffix_and_skips_ratio_for_zero() {
        let value = serde_json::json!({
            "data": { "stream": { "baseSteamInfoList": [
                { "sFlvUrl": "http://tx.flv.huya.com/src", "sStreamName": "s",
                  "sFlvUrlSuffix": "", "sFlvAntiCode": "wsSecret=1" },
                { "sFlvUrl": "", "sStreamName": "ignored", "sFlvAntiCode": "x" }
            ]}}
        });
        let urls = flv_urls(&value, 0);
        assert_eq!(urls, vec!["http://tx.flv.huya.com/src/s.flv?wsSecret=1"]);
    }

    #[test]
    fn parses_quality_array_and_skips_hdr() {
        let value = serde_json::json!({
            "data": { "stream": { "hls": { "rateArray": [
                { "sDisplayName": "蓝光8M", "iBitRate": 8000 },
                { "sDisplayName": "HDR10", "iBitRate": 6000 },
                { "sDisplayName": "蓝光4M", "iBitRate": 4000 },
                { "sDisplayName": "超清", "iBitRate": "2000" },
                { "sDisplayName": "", "iBitRate": 500 }
            ]}}}
        });
        let qualities = parse_qualities(&value);
        assert_eq!(
            qualities.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(),
            vec!["蓝光8M", "蓝光4M", "超清"]
        );
        assert_eq!(qualities[2].qn, 2000);
    }

    /// 真实网络冒烟测试: `cargo test --lib huya -- --ignored`。
    ///
    /// 断言取到的是 FLV 线路(虎牙 HLS 对 Web 端已恒定 403), 且至少一条能连上。
    /// 同一线路会随机 403, 因此每条线路允许重试一次。
    #[tokio::test]
    #[ignore]
    async fn huya_returns_playable_flv_lines() {
        use crate::crawler::http_client::HttpClient;
        let http = HttpClient::new(15, None).expect("http client");
        let huya = Huya::new(http.clone());
        // 房间号需自行替换为正在直播的房间(998 只是常用测试间)。
        let info = huya.stream_urls("998", None).await.expect("stream info");
        assert!(info.is_live, "房间未开播, 请换一个正在直播的房间号");
        assert!(!info.streams.is_empty(), "没有取到任何线路");
        assert!(
            info.streams.iter().all(|item| item.format == "flv"),
            "虎牙应只下发 FLV"
        );

        let mut connected = false;
        'outer: for stream in info.streams.iter().take(4) {
            for _ in 0..2 {
                if let Ok(response) = http
                    .client()
                    .get(&stream.url)
                    .header(reqwest::header::USER_AGENT, USER_AGENT)
                    .timeout(std::time::Duration::from_secs(8))
                    .send()
                    .await
                {
                    if response.status().is_success() {
                        connected = true;
                        break 'outer;
                    }
                }
            }
        }
        assert!(connected, "所有 FLV 线路都连不上");
    }
}
