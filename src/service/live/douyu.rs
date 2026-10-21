//! 斗鱼直播适配。
//!
//! 分区/列表/详情都是公开 JSON 接口; 取流需要运行斗鱼下发的混淆 JS 算签名:
//! 1. `swf_api/homeH5Enc?rids={rid}` 拿到 `ub98484234` 所在的加密脚本;
//! 2. 用 `rquickjs` 先加载 crypto-js, 再执行该脚本, 调 `ub98484234(rid,did,tt)`;
//! 3. POST `lapi/live/getH5Play/{rid}` 得到 `.flv` 地址。
//!
//! 该 FLV 地址实测**不校验 Referer**、CDN 回 `Access-Control-Allow-Origin: *`,
//! 因此 WebView 内可直接用 flv.js 播放, 不需要本地代理。

use futures::future::join_all;
use rquickjs::{Context, Runtime};
use serde_json::Value;
use std::time::Duration;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use super::models::{
    LiveCategory, LiveCategoryGroup, LiveRoomDetail, LiveRoomSummary, LiveStream, LiveStreamInfo,
};

pub const PLATFORM_ID: &str = "douyu";
pub const PLATFORM_NAME: &str = "斗鱼直播";

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
/// 斗鱼签名脚本要求的设备号, 固定值即可(与 simple_live 一致)。
const DEVICE_ID: &str = "10000000000000000000000000001501";
/// 页面版本号, 与 simple_live 保持一致; 斗鱼只校验格式不校验真伪。
const PLAY_VER: &str = "Douyu_223061205";

const CRYPTO_JS: &str = include_str!("assets/cryptojs.js");

pub struct Douyu {
    http: HttpClient,
}

impl Douyu {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn categories(&self) -> Result<Vec<LiveCategoryGroup>, AppError> {
        let value = self
            .get_json("https://m.douyu.com/api/cate/list", "https://m.douyu.com/")
            .await?;
        Ok(parse_categories(&value))
    }

    pub async fn rooms(
        &self,
        parent: &str,
        area: &str,
        page: u32,
    ) -> Result<Vec<LiveRoomSummary>, AppError> {
        let page = page.max(1);
        // area 为 0/空 = 清空筛选: 斗鱼的 directory 接口 1_<cate1> 能直接返回整个
        // 一级分区的房间(与网页「全部」一致); 落到二级分区时用 2_<cate2>。
        let url = if area.trim().is_empty() || area.trim() == "0" {
            let cate1 = normalize_id(parent, "一级分区")?;
            format!("https://www.douyu.com/gapi/rkc/directory/mixList/1_{cate1}/{page}")
        } else {
            let cate2 = normalize_id(area, "二级分区")?;
            format!("https://www.douyu.com/gapi/rkc/directory/mixList/2_{cate2}/{page}")
        };
        let value = self.get_json(&url, "https://www.douyu.com/").await?;
        Ok(parse_rooms(&value))
    }

    pub async fn room_detail(&self, room_id: &str) -> Result<LiveRoomDetail, AppError> {
        let id = normalize_id(room_id, "房间号")?;
        // betard 是详情主体; h5room 只为拿 `show_time`(开播时间), 失败不影响详情。
        let room = self.betard(&id);
        let show_time = async {
            match self
                .get_json(
                    &format!("https://www.douyu.com/swf_api/h5room/{id}"),
                    &format!("https://www.douyu.com/{id}"),
                )
                .await
            {
                Ok(value) => value
                    .pointer("/data/show_time")
                    .cloned()
                    .unwrap_or(Value::Null),
                Err(_) => Value::Null,
            }
        };
        let (value, show_time) = tokio::try_join!(room, async { Ok::<_, AppError>(show_time.await) })?;
        let start_time = show_time
            .as_i64()
            .or_else(|| show_time.as_str().and_then(|s| s.parse().ok()))
            .filter(|v| *v > 0)
            .unwrap_or(0);
        parse_room_detail(&value, &id, start_time)
            .ok_or_else(|| AppError::NotFound(format!("直播间 {id} 不存在")))
    }

    pub async fn stream_urls(&self, room_id: &str) -> Result<LiveStreamInfo, AppError> {
        let id = normalize_id(room_id, "房间号")?;
        let room = self.betard(&id).await?;
        let info = room.get("room").cloned().unwrap_or(Value::Null);
        // `videoLoop == 1` 是轮播录像, 不算直播。
        let is_live = info.get("show_status").and_then(Value::as_i64) == Some(1)
            && info.get("videoLoop").and_then(Value::as_i64) != Some(1);
        if !is_live {
            return Ok(LiveStreamInfo {
                room_id: id,
                is_live: false,
                streams: Vec::new(),
            });
        }

        let sign = self.sign_query(&id).await?;
        // 与 simple_live 一致: 先用空 cdn 拿到 multirates 与 cdnsWithName,
        // 再逐个 CDN 取每个码率的播放地址。
        let first = self.play_info(&id, &sign, -1, "").await?;
        // 清晰度列表直接沿用平台下发的 multirates(顺序即 高 -> 低),
        // 不重排、不去重, 与 simple_live 的「切换清晰度」弹窗完全一致。
        let rates = parse_rates(&first);

        let mut cdns = parse_cdns(&first);
        // scdn 线路放最后(与 simple_live 一致); 没有可用线路时保留空 cdn 的旧行为。
        cdns.sort_by(|a, b| a.starts_with("scdn").cmp(&b.starts_with("scdn")));
        if cdns.is_empty() {
            cdns.push(String::new());
        }

        let others: Vec<_> = rates
            .iter()
            .filter(|(rate, _)| *rate >= 0)
            .map(|(rate, name)| {
                let rate = *rate;
                let name = name.clone();
                let signs = sign.clone();
                let id = id.clone();
                let cdn_list = cdns.clone();
                async move {
                    // 多线路容灾: 每个码率最多收集 2 条可用 CDN, 失败时前端自动换线。
                    let mut found: Vec<LiveStream> = Vec::new();
                    for cdn in cdn_list {
                        if found.len() >= 2 {
                            break;
                        }
                        if let Ok(value) = self.play_rate(&id, &signs, rate, &cdn).await {
                            if has_rtmp_live(&value) {
                                found.push(stream_from(&value, &name));
                            }
                        }
                    }
                    if found.is_empty() {
                        None
                    } else {
                        Some(found)
                    }
                }
            })
            .collect();
        let mut streams: Vec<LiveStream> = Vec::new();
        for list in join_all(others).await.into_iter().flatten() {
            streams.extend(list);
        }
        // 所有码率都取不到时退回默认线路, 避免整个房间直接判为失败。
        if streams.is_empty() {
            if has_rtmp_live(&first) {
                streams.push(stream_from(&first, "默认"));
            }
        }
        Ok(LiveStreamInfo {
            room_id: id,
            is_live: true,
            streams,
        })
    }

    pub async fn search(&self, keyword: &str, page: u32) -> Result<Vec<LiveRoomSummary>, AppError> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(AppError::BadRequest("搜索关键词不能为空".to_string()));
        }
        let url = format!(
            "https://www.douyu.com/japi/search/api/searchShow?kw={}&page={}&pageSize=20",
            urlencoding::encode(keyword),
            page.max(1)
        );
        let response = self
            .http
            .client()
            .get(&url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, "https://www.douyu.com/search/")
            .header(
                reqwest::header::COOKIE,
                format!("dy_did={DEVICE_ID}; acf_did={DEVICE_ID}"),
            )
            .timeout(Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?;
        let value: Value = response.json().await?;
        match value.get("error").and_then(Value::as_i64).unwrap_or(0) {
            0 => Ok(parse_search(&value)),
            code => Err(AppError::BadRequest(format!(
                "斗鱼搜索失败({code}): {}",
                text(&value, "msg")
            ))),
        }
    }

    async fn betard(&self, room_id: &str) -> Result<Value, AppError> {
        let url = format!("https://www.douyu.com/betard/{room_id}");
        self.get_json(&url, &format!("https://www.douyu.com/{room_id}"))
            .await
    }

    async fn play_info(
        &self,
        room_id: &str,
        sign: &str,
        rate: i64,
        cdn: &str,
    ) -> Result<Value, AppError> {
        let body = format!("{sign}&cdn={cdn}&rate={rate}&ver={PLAY_VER}&iar=1&ive=1&hevc=0&fa=0");
        self.post_play(room_id, body).await
    }

    /// 按清晰度取流。请求体必须与 simple_live 完全一致, 只带 `sign + cdn + rate`;
    /// 多带 `ver/iar/ive/hevc/fa` 会让斗鱼把 rate 映射到错误档位
    /// (实测「蓝光4M」回退成 _900 标清, 而只带 rate 时能正确返回 _4000)。
    async fn play_rate(
        &self,
        room_id: &str,
        sign: &str,
        rate: i64,
        cdn: &str,
    ) -> Result<Value, AppError> {
        let body = format!("{sign}&cdn={cdn}&rate={rate}");
        self.post_play(room_id, body).await
    }

    async fn post_play(&self, room_id: &str, body: String) -> Result<Value, AppError> {
        let url = format!("https://www.douyu.com/lapi/live/getH5Play/{room_id}");
        let response = self
            .http
            .client()
            .post(&url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, format!("https://www.douyu.com/{room_id}"))
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(reqwest::header::COOKIE, format!("dy_did={DEVICE_ID}; acf_did={DEVICE_ID}"))
            .body(body)
            .timeout(Duration::from_secs(10))
            .send()
            .await?
            .error_for_status()?;
        let value: Value = response.json().await?;
        match value.get("error").and_then(Value::as_i64).unwrap_or(0) {
            0 => Ok(value),
            code => Err(AppError::BadRequest(format!(
                "斗鱼取流失败({code}): {}",
                text(&value, "msg")
            ))),
        }
    }

    /// 取签名查询串: 下载加密脚本 → 在独立 QuickJS 里算 `ub98484234(...)`。
    async fn sign_query(&self, room_id: &str) -> Result<String, AppError> {
        let url = format!("https://www.douyu.com/swf_api/homeH5Enc?rids={room_id}");
        let value = self.get_json(&url, &format!("https://www.douyu.com/{room_id}")).await?;
        let script = value
            .pointer(&format!("/data/room{room_id}"))
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| AppError::BadRequest("斗鱼未返回签名脚本".to_string()))?
            .to_string();
        let id = room_id.to_string();
        tokio::task::spawn_blocking(move || run_sign(&script, &id))
            .await
            .map_err(|err| AppError::Internal(anyhow::anyhow!("签名线程失败: {err}")))?
            .map_err(|err| AppError::BadRequest(format!("斗鱼签名失败: {err}")))
    }

    async fn get_json(&self, url: &str, referer: &str) -> Result<Value, AppError> {
        let response = self
            .http
            .client()
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, referer)
            .timeout(Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
    }
}

/// 在独立 QuickJS 上下文里执行 crypto-js + 斗鱼脚本, 返回签名查询串。
fn run_sign(script: &str, room_id: &str) -> anyhow::Result<String> {
    let runtime = Runtime::new()?;
    runtime.set_max_stack_size(4 * 1024 * 1024);
    let context = Context::full(&runtime)?;
    let tt = chrono::Utc::now().timestamp();
    context.with(|ctx| {
        ctx.eval::<(), _>(CRYPTO_JS.as_bytes())?;
        ctx.eval::<(), _>(script.as_bytes())?;
        let call = format!("ub98484234('{room_id}','{DEVICE_ID}',{tt})");
        let value: String = ctx.eval(call.as_bytes())?;
        Ok(value)
    })
}

fn has_rtmp_live(value: &Value) -> bool {
    value
        .pointer("/data/rtmp_live")
        .and_then(Value::as_str)
        .is_some_and(|live| !live.is_empty())
}

/// `multirates` 的顺序就是平台给的画质顺序(高 -> 低), 保持原样。
fn parse_rates(value: &Value) -> Vec<(i64, String)> {
    value
        .pointer("/data/multirates")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let rate = item.get("rate").and_then(Value::as_i64)?;
                    Some((rate, text(item, "name")))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_cdns(value: &Value) -> Vec<String> {
    value
        .pointer("/data/cdnsWithName")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|item| item.get("cdn").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn stream_from(value: &Value, quality: &str) -> LiveStream {
    let host = value
        .pointer("/data/rtmp_url")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let live = value
        .pointer("/data/rtmp_live")
        .and_then(Value::as_str)
        .unwrap_or_default();
    LiveStream {
        format: "flv".to_string(),
        quality: quality.to_string(),
        url: if host.is_empty() || live.is_empty() {
            String::new()
        } else {
            format!("{host}/{live}")
        },
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

pub fn parse_categories(value: &Value) -> Vec<LiveCategoryGroup> {
    let Some(parents) = value.pointer("/data/cate1Info").and_then(Value::as_array) else {
        return Vec::new();
    };
    let children = value
        .pointer("/data/cate2Info")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    parents
        .iter()
        .filter_map(|parent| {
            let id = text(parent, "cate1Id");
            if id.is_empty() {
                return None;
            }
            let subs = children
                .iter()
                .filter(|child| text(child, "cate1Id") == id)
                .filter_map(|child| {
                    let sub_id = text(child, "cate2Id");
                    if sub_id.is_empty() {
                        return None;
                    }
                    Some(LiveCategory {
                        id: sub_id,
                        name: text(child, "cate2Name"),
                        icon: {
                            let raw = text(child, "icon");
                            if raw.is_empty() {
                                text(child, "smallIcon")
                            } else {
                                raw
                            }
                        },
                    })
                })
                .collect();
            Some(LiveCategoryGroup {
                id,
                name: text(parent, "cate1Name"),
                children: subs,
            })
        })
        .collect()
}

pub fn parse_rooms(value: &Value) -> Vec<LiveRoomSummary> {
    let Some(items) = value.pointer("/data/rl").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_i64) == Some(1))
        .filter_map(|item| {
            let room_id = text(item, "rid");
            if room_id.is_empty() || room_id == "0" {
                return None;
            }
            let cover = ["rs16", "rs1"]
                .iter()
                .map(|key| text(item, key))
                .find(|value| !value.is_empty())
                .unwrap_or_default();
            Some(LiveRoomSummary {
                platform: PLATFORM_ID.to_string(),
                room_id,
                title: text(item, "rn"),
                cover,
                user_name: text(item, "nn"),
                online: number(item, "ol"),
                area_name: text(item, "c2name"),
            })
        })
        .collect()
}

/// 搜索结果: `data.relateShow[]`, `hot` 可能是 "12.3万" 这种带单位的字符串。
pub fn parse_search(value: &Value) -> Vec<LiveRoomSummary> {
    let Some(items) = value
        .pointer("/data/relateShow")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let room_id = text(item, "rid");
            if room_id.is_empty() || room_id == "0" {
                return None;
            }
            Some(LiveRoomSummary {
                platform: PLATFORM_ID.to_string(),
                room_id,
                title: text(item, "roomName"),
                cover: text(item, "roomSrc"),
                user_name: text(item, "nickName"),
                online: parse_hot(&text(item, "hot")),
                area_name: text(item, "cateName"),
            })
        })
        .collect()
}

/// "12.3万" → 123000; 纯数字原样解析; 解析失败按 0。
fn parse_hot(raw: &str) -> u64 {
    let raw = raw.trim();
    if let Some(head) = raw.strip_suffix('万') {
        return head
            .parse::<f64>()
            .map(|value| (value * 10_000.0) as u64)
            .unwrap_or(0);
    }
    raw.parse::<u64>().unwrap_or(0)
}

pub fn parse_room_detail(value: &Value, room_id: &str, live_start_time: i64) -> Option<LiveRoomDetail> {
    let room = value.get("room")?;
    let id = text(room, "room_id");
    if id.is_empty() || id == "0" {
        return None;
    }
    Some(LiveRoomDetail {
        platform: PLATFORM_ID.to_string(),
        room_id: room_id.to_string(),
        title: text(room, "room_name"),
        cover: text(room, "room_pic"),
        user_name: text(room, "owner_name"),
        avatar: text(room, "owner_avatar"),
        online: number(room.pointer("/room_biz_all").unwrap_or(&Value::Null), "hot"),
        followers: 0,
        description: text(room, "show_details"),
        area_name: text(room, "second_lvl_name"),
        parent_area_name: String::new(),
        is_live: room.get("show_status").and_then(Value::as_i64) == Some(1)
            && room.get("videoLoop").and_then(Value::as_i64) != Some(1),
        live_start_time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cryptojs_asset_is_loadable() {
        let runtime = Runtime::new().expect("runtime");
        let context = Context::full(&runtime).expect("context");
        context.with(|ctx| {
            ctx.eval::<(), _>(CRYPTO_JS).expect("eval crypto-js");
            let digest: String = ctx.eval("CryptoJS.MD5('abc').toString()").expect("md5");
            assert_eq!(digest, "900150983cd24fb0d6963f7d28e17f72");
        });
    }

    /// 签名脚本来自网络, 这里用一个最小替身守住「加载 crypto-js → 执行脚本 →
    /// 取返回值」这条链路(真实脚本 49 KB, 不适合放进仓库)。
    #[test]
    fn runs_signature_script_in_sandbox() {
        let script = "function ub98484234(rid,did,tt){return 'v=1&did='+did+'&tt='+tt+'&sign='+CryptoJS.MD5(rid+did+tt).toString()}";
        let query = run_sign(script, "317422").expect("sign");
        assert!(query.starts_with("v=1&did="), "query: {query}");
        assert!(query.contains("sign="), "query: {query}");
        assert!(query.contains(&format!("did={DEVICE_ID}")), "query: {query}");
    }

    /// 真实链路: 抓斗鱼下发的 49 KB 混淆脚本, 在 QuickJS 里跑出签名, 再换一次流地址。
    /// 需要外网, 默认忽略(`cargo test --lib -- --ignored service::live::douyu`)。
    #[tokio::test]
    #[ignore]
    async fn signs_and_fetches_a_real_stream() {
        let http = HttpClient::new(30, None).expect("http");
        let douyu = Douyu::new(http);
        // 房间号写死会踩「主播未开播」, 先搜一个当前在播的房间。
        let rooms = douyu.search("英雄联盟", 1).await.expect("search live rooms");
        let room_id = rooms
            .first()
            .map(|room| room.room_id.clone())
            .expect("at least one live douyu room");
        let info = douyu
            .stream_urls(&room_id)
            .await
            .expect("stream urls");
        assert!(info.is_live, "searched room {room_id} should be live");
        assert!(!info.streams.is_empty(), "should expose at least one stream");
        assert_eq!(info.streams[0].format, "flv");
        assert!(info.streams[0].url.contains(".flv"), "{}", info.streams[0].url);
    }

    #[test]
    fn parses_categories_by_parent_id() {
        let value = serde_json::json!({
            "data": {
                "cate1Info": [{ "cate1Id": 1, "cate1Name": "网游竞技" }],
                "cate2Info": [
                    { "cate1Id": 1, "cate2Id": 1, "cate2Name": "英雄联盟" },
                    { "cate1Id": 2, "cate2Id": 2, "cate2Name": "其他" }
                ]
            }
        });
        let groups = parse_categories(&value);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "网游竞技");
        assert_eq!(groups[0].children.len(), 1);
        assert_eq!(groups[0].children[0].name, "英雄联盟");
    }

    #[test]
    fn parses_rooms_and_skips_non_live_entries() {
        let value = serde_json::json!({
            "data": { "rl": [
                { "type": 1, "rid": 317422, "rn": "标题", "nn": "主播", "ol": 2772923,
                  "rs16": "https://rpic.douyucdn.cn/a.png/dy1", "c2name": "英雄联盟" },
                { "type": 2, "rid": 1, "rn": "广告" }
            ]}
        });
        let rooms = parse_rooms(&value);
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "317422");
        assert_eq!(rooms[0].online, 2772923);
    }

    #[test]
    fn parses_search_results_and_hot_numbers() {
        let value = serde_json::json!({
            "data": { "relateShow": [
                { "rid": 317422, "roomName": "标题", "roomSrc": "https://rpic.douyucdn.cn/a.png",
                  "nickName": "主播", "hot": "12.3万", "cateName": "英雄联盟" },
                { "rid": 2, "roomName": "数字", "hot": "4567" }
            ]}
        });
        let rooms = parse_search(&value);
        assert_eq!(rooms.len(), 2);
        assert_eq!(rooms[0].online, 123000);
        assert_eq!(rooms[0].user_name, "主播");
        assert_eq!(rooms[1].online, 4567);
    }

    #[test]
    fn treats_loop_video_as_offline() {
        let value = serde_json::json!({
            "room": { "room_id": 1, "room_name": "轮播", "show_status": 1, "videoLoop": 1 }
        });
        let detail = parse_room_detail(&value, "1", 1760000000).expect("parse");
        assert!(!detail.is_live);
        assert_eq!(detail.live_start_time, 1760000000);
    }

    #[test]
    fn builds_flv_url_from_parts() {
        let value = serde_json::json!({
            "data": { "rtmp_url": "https://huos3.douyucdn2.cn/live", "rtmp_live": "1abc.flv?x=1" }
        });
        let stream = stream_from(&value, "原画");
        assert_eq!(stream.format, "flv");
        assert_eq!(stream.quality, "原画");
        assert_eq!(stream.url, "https://huos3.douyucdn2.cn/live/1abc.flv?x=1");
    }

    /// 清晰度列表必须原样保留平台下发的 multirates 顺序与名称,
    /// 这样同一个房间能像 simple_live 一样显示全部档位(原画/蓝光/超清/高清)。
    #[test]
    fn keeps_platform_quality_order() {
        let value = serde_json::json!({
            "data": {
                "multirates": [
                    { "name": "原画1080P60", "rate": 0 },
                    { "name": "蓝光4M", "rate": 4 },
                    { "name": "超清", "rate": 3 },
                    { "name": "高清", "rate": 2 }
                ],
                "cdnsWithName": [
                    { "cdn": "hw-h5" },
                    { "cdn": "scdn-h5" }
                ]
            }
        });
        let rates = parse_rates(&value);
        let names: Vec<&str> = rates.iter().map(|(_, name)| name.as_str()).collect();
        assert_eq!(names, ["原画1080P60", "蓝光4M", "超清", "高清"]);
        assert_eq!(rates.iter().map(|(rate, _)| *rate).collect::<Vec<_>>(), [0, 4, 3, 2]);
        assert_eq!(parse_cdns(&value), ["hw-h5", "scdn-h5"]);
    }
}
