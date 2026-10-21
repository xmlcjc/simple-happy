//! 虎牙直播适配。
//!
//! 全部使用公开 Web 接口, 无需登录/签名:
//! - 分区: `live.cdn.huya.com/liveconfig/game/bussLive`
//! - 房间列表: `www.huya.com/cache.php?m=LiveList&do=getLiveListByPage`
//! - 房间详情 + 取流: `mp.huya.com/cache.php?m=Live&do=profileRoom`
//!
//! 取流返回的 HLS 地址带时效签名(`wsSecret`), 直接可用; 但虎牙有多条 CDN,
//! 其中 AL 线经常 502, 因此这里会按顺序探测, 只返回真正能拉到的地址。

use serde_json::Value;
use std::time::Duration;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use super::models::{
    LiveCategory, LiveCategoryGroup, LiveRoomDetail, LiveRoomSummary, LiveStream, LiveStreamInfo,
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

    pub async fn stream_urls(&self, room_id: &str) -> Result<LiveStreamInfo, AppError> {
        let id = normalize_id(room_id, "房间号")?;
        let value = self.profile(&id).await?;
        let is_live = value.pointer("/data/liveStatus").and_then(Value::as_str) == Some("ON");
        let mut streams = Vec::new();
        if is_live {
            for url in candidate_hls_urls(&value) {
                if self.is_reachable(&url).await {
                    streams.push(LiveStream {
                        format: "hls".to_string(),
                        quality: "原画".to_string(),
                        url,
                    });
                    break;
                }
            }
        }
        Ok(LiveStreamInfo {
            room_id: id,
            is_live,
            streams,
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

    /// 只取响应头, 判断该 CDN 线路是否真的可用。
    async fn is_reachable(&self, url: &str) -> bool {
        let request = self
            .http
            .client()
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, "https://www.huya.com/")
            .timeout(Duration::from_secs(6))
            .send();
        matches!(request.await, Ok(response) if response.status().is_success())
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
    })
}

/// 由 `baseSteamInfoList` 拼出候选 HLS 地址, 按线路优先级排序。
/// 优先 https, 其次 http; 同一线路的两种协议都会给出, 由 `is_reachable` 决定。
pub fn candidate_hls_urls(value: &Value) -> Vec<String> {
    let Some(items) = value.pointer("/data/stream/baseSteamInfoList").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut urls = Vec::new();
    for item in items {
        let host = text(item, "sHlsUrl");
        let name = text(item, "sStreamName");
        let suffix = text(item, "sHlsUrlSuffix");
        let anti = text(item, "sHlsAntiCode");
        if host.is_empty() || name.is_empty() || suffix.is_empty() {
            continue;
        }
        let base = format!("{host}/{name}.{suffix}");
        let https = base
            .strip_prefix("http://")
            .map(|rest| format!("https://{rest}"))
            .unwrap_or_else(|| base.clone());
        if anti.is_empty() {
            urls.push(https);
            urls.push(base);
        } else {
            urls.push(format!("{https}?{anti}"));
            urls.push(format!("{base}?{anti}"));
        }
    }
    urls
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
    fn builds_candidate_urls_with_https_first() {
        let value = serde_json::json!({
            "data": { "stream": { "baseSteamInfoList": [
                { "sHlsUrl": "http://al.hls.huya.com/src", "sStreamName": "a-b-c",
                  "sHlsUrlSuffix": "m3u8", "sHlsAntiCode": "wsSecret=1&wsTime=2" }
            ]}}
        });
        let urls = candidate_hls_urls(&value);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://al.hls.huya.com/src/a-b-c.m3u8?wsSecret=1&wsTime=2");
        assert_eq!(urls[1], "http://al.hls.huya.com/src/a-b-c.m3u8?wsSecret=1&wsTime=2");
    }
}
