//! 抖音直播适配。
//!
//! 列表/详情接口需要 `a_bogus` 签名: 与 simple_live 相同, 用 `rquickjs` 跑官方
//! 混淆 JS(`assets/douyin_abogus.js`); 弹幕签名则是 `assets/douyin_webmssdk.js`
//! (见 `danmaku/douyin.rs`)。
//!
//! - 分区: 抓 `live.douyin.com` 首页 HTML 里的转义 JSON(`categoryData`);
//! - 房间列表: `webcast/web/partition/detail/room/v2/`(a_bogus);
//! - 详情/取流: `webcast/room/web/enter/`(a_bogus), 19 位 roomId 走
//!   `webcast.amemv.com/webcast/room/reflow/info/`(免签名);
//! - 搜索: `www.douyin.com/aweme/v1/web/live/search/`(不签名, 但要先 HEAD
//!   主站拿 ttwid/__ac_nonce cookie)。

use md5::Digest;
use regex::Regex;
use rquickjs::{Context, Runtime};
use serde_json::Value;
use std::time::Duration;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use super::models::{
    LiveCategory, LiveCategoryGroup, LiveRoomDetail, LiveRoomSummary, LiveStream, LiveStreamInfo,
};

pub const PLATFORM_ID: &str = "douyin";
pub const PLATFORM_NAME: &str = "抖音直播";

/// QQBrowser UA(与 simple_live / DouyinLiveRecorder 一致, 抖音风控认这个)。
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.5845.97 Safari/537.36 Core/1.116.567.400 QQBrowser/19.7.6764.400";
const REFERER: &str = "https://live.douyin.com";
/// 兜底 ttwid(simple_live 实测只用 ttwid 即可拿到全部画质)。
const DEFAULT_COOKIE: &str = "ttwid=1%7CB1qls3GdnZhUov9o2NxOMxxYS2ff6OSvEWbv0ytbES4%7C1680522049%7C280d802d6d478e3e78d0c807f7c487e7ffec0ae4e5fdd6a0fe74c3c6af149511";

const ABOGUS_JS: &str = include_str!("assets/douyin_abogus.js");
const WEBMSSDK_JS: &str = include_str!("assets/douyin_webmssdk.js");

pub struct Douyin {
    http: HttpClient,
}

impl Douyin {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn categories(&self) -> Result<Vec<LiveCategoryGroup>, AppError> {
        let html = self.get_text(REFERER, DEFAULT_COOKIE, REFERER).await?;
        Ok(parse_categories(&html))
    }

    pub async fn rooms(
        &self,
        parent: &str,
        area: &str,
        page: u32,
    ) -> Result<Vec<LiveRoomSummary>, AppError> {
        // 分区 id 形如 "{id},{type}"; 没选子分区时退到推荐(720,1)。
        let id = if area.contains(',') {
            area
        } else if parent.contains(',') {
            parent
        } else {
            "720,1"
        };
        let mut parts = id.splitn(2, ',');
        let partition = parts.next().unwrap_or("720").trim();
        let partition_type = parts.next().unwrap_or("1").trim();
        if partition.is_empty() || !partition.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::BadRequest(format!("分区无效: {id}")));
        }
        let offset = (page.max(1) - 1) * 15;
        let url = format!(
            "https://live.douyin.com/webcast/web/partition/detail/room/v2/?aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&enter_from=link_share&cookie_enabled=true&screen_width=1980&screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Edge&browser_version=125.0.0.0&browser_online=true&count=15&offset={offset}&partition={partition}&partition_type={partition_type}&req_from=2"
        );
        let signed = abogus_url(&url).await?;
        let value = self.get_json(&signed, DEFAULT_COOKIE, REFERER).await?;
        Ok(parse_partition_rooms(&value))
    }

    pub async fn search(&self, keyword: &str, page: u32) -> Result<Vec<LiveRoomSummary>, AppError> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(AppError::BadRequest("搜索关键词不能为空".to_string()));
        }
        // 搜索在 www.douyin.com 域下, 需要先 HEAD 主站拿 ttwid/__ac_nonce。
        let cookie = self.fresh_cookie("https://live.douyin.com").await;
        let offset = (page.max(1) - 1) * 10;
        let url = format!(
            "https://www.douyin.com/aweme/v1/web/live/search/?device_platform=webapp&aid=6383&channel=channel_pc_web&search_channel=aweme_live&keyword={}&search_source=switch_tab&query_correct_type=1&is_filter_search=0&from_group_id=&offset={offset}&count=10&pc_client_type=1&version_code=170400&version_name=17.4.0&cookie_enabled=true&screen_width=1980&screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Edge&browser_version=125.0.0.0&browser_online=true&engine_name=Blink&engine_version=125.0.0.0&os_name=Windows&os_version=10&cpu_core_num=12&device_memory=8&platform=PC&downlink=10&effective_type=4g&round_trip_time=100&webid=7382872326016435738",
            urlencoding::encode(keyword)
        );
        let referer = format!(
            "https://www.douyin.com/search/{}?type=live",
            urlencoding::encode(keyword)
        );
        // 2026 年起搜索接口风控收紧: 未签名请求一律返回 status_code=2483(要求登录)。
        let signed = abogus_url(&url).await?;
        let result = self.get_json(&signed, &cookie, &referer).await;
        // 风控时返回空 body / "blocked", 统一换成可读错误。
        match result {
            Ok(value) => {
                // 抖音搜索已全面要求登录(status_code=2483), 把平台原话抛给用户。
                let code = value.get("status_code").and_then(Value::as_i64).unwrap_or(0);
                if code != 0 {
                    let msg = value
                        .get("status_msg")
                        .and_then(Value::as_str)
                        .unwrap_or("搜索失败");
                    return Err(AppError::BadRequest(format!("抖音搜索不可用: {msg}")));
                }
                Ok(parse_search(&value))
            }
            Err(_) => Err(AppError::BadRequest(
                "抖音直播搜索被限制，请稍后再试".to_string(),
            )),
        }
    }

    pub async fn room_detail(&self, room_id: &str) -> Result<LiveRoomDetail, AppError> {
        let id = normalize_id(room_id, "房间号")?;
        // webRid(≤16 位)走 enter API, 失败回退房间页 HTML; 19 位 roomId 走 reflow。
        if id.len() <= 16 {
            if let Ok(data) = self.enter_data(&id).await {
                if let Some(detail) = parse_enter_detail(&data, &id) {
                    return Ok(detail);
                }
            }
            let state = self.room_state_by_html(&id).await?;
            return parse_html_detail(&state, &id)
                .ok_or_else(|| AppError::NotFound(format!("直播间 {id} 不存在")));
        }
        let value = self.reflow_data(&id).await?;
        let status = value
            .pointer("/data/room/status")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        // roomId 是一次性的: 已下播(status=4)时改按 webRid 重查。
        if status == 4 {
            let web_rid = value
                .pointer("/data/room/owner")
                .map(|owner| text(owner, "web_rid"))
                .unwrap_or_default();
            if !web_rid.is_empty() && web_rid != id {
                return Box::pin(self.room_detail(&web_rid)).await;
            }
        }
        parse_reflow_detail(&value, &id)
            .ok_or_else(|| AppError::NotFound(format!("直播间 {id} 不存在")))
    }

    pub async fn stream_urls(&self, room_id: &str) -> Result<LiveStreamInfo, AppError> {
        let id = normalize_id(room_id, "房间号")?;
        let mut is_live = false;
        let mut stream_url = Value::Null;
        if id.len() <= 16 {
            if let Ok(data) = self.enter_data(&id).await {
                if let Some(room) = data.get("data").and_then(Value::as_array).and_then(|a| a.first()) {
                    is_live = room.get("status").and_then(Value::as_i64) == Some(2);
                    if is_live {
                        stream_url = room.get("stream_url").cloned().unwrap_or(Value::Null);
                    }
                }
            }
            // enter API 失败时回退 HTML(同样能拿到 stream_url)。
            if stream_url.is_null() {
                if let Ok(state) = self.room_state_by_html(&id).await {
                    let room = state.pointer("/roomStore/roomInfo/room").cloned().unwrap_or(Value::Null);
                    is_live = room.get("status").and_then(Value::as_i64) == Some(2);
                    if is_live {
                        stream_url = room.get("stream_url").cloned().unwrap_or(Value::Null);
                    }
                }
            }
        } else {
            let value = self.reflow_data(&id).await?;
            let room = value.pointer("/data/room").cloned().unwrap_or(Value::Null);
            let status = room.get("status").and_then(Value::as_i64).unwrap_or(0);
            if status == 4 {
                let web_rid = room
                    .get("owner")
                    .map(|owner| text(owner, "web_rid"))
                    .unwrap_or_default();
                if !web_rid.is_empty() && web_rid != id {
                    return Box::pin(self.stream_urls(&web_rid)).await;
                }
            }
            is_live = status == 2;
            if is_live {
                stream_url = room.get("stream_url").cloned().unwrap_or(Value::Null);
            }
        }
        let streams = if is_live {
            parse_streams(&stream_url)
        } else {
            Vec::new()
        };
        Ok(LiveStreamInfo {
            room_id: id,
            is_live,
            streams,
            qualities: Vec::new(),
        })
    }

    /// `webcast/room/web/enter/` 返回的 `data` 节点(含 `data[0]` 房间与 `user`)。
    /// 弹幕模块也靠它把 webRid 换算成真实 roomId。
    pub(crate) async fn enter_data(&self, web_rid: &str) -> Result<Value, AppError> {
        let url = format!(
            "https://live.douyin.com/webcast/room/web/enter/?aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=125.0.0.0&web_rid={web_rid}&msToken="
        );
        let signed = abogus_url(&url).await?;
        let referer = format!("{REFERER}/{web_rid}");
        let value = self.get_json(&signed, DEFAULT_COOKIE, &referer).await?;
        value
            .get("data")
            .cloned()
            .filter(|data| data.is_object())
            .ok_or_else(|| AppError::BadRequest("抖音接口返回格式异常".to_string()))
    }

    /// 19 位 roomId 的详情接口, 免签名。
    async fn reflow_data(&self, room_id: &str) -> Result<Value, AppError> {
        let url = format!(
            "https://webcast.amemv.com/webcast/room/reflow/info/?type_id=0&live_id=1&room_id={room_id}&sec_user_id=&version_code=99.99.99&app_id=6383"
        );
        self.get_json(&url, DEFAULT_COOKIE, REFERER).await
    }

    /// 房间页 HTML 里的 `state` 节点(enter API 被风控时的后备)。
    async fn room_state_by_html(&self, web_rid: &str) -> Result<Value, AppError> {
        let page_url = format!("{REFERER}/{web_rid}");
        let cookie = self.fresh_cookie(&page_url).await;
        let html = self.get_text(&page_url, &cookie, REFERER).await?;
        parse_html_state(&html)
            .ok_or_else(|| AppError::BadRequest("未能从房间页解析直播间信息".to_string()))
    }

    /// HEAD 主站/房间页, 从 set-cookie 收集 ttwid/__ac_nonce/msToken; 失败用兜底 ttwid。
    async fn fresh_cookie(&self, url: &str) -> String {
        fresh_cookie(&self.http, url).await
    }

    async fn get_json(&self, url: &str, cookie: &str, referer: &str) -> Result<Value, AppError> {
        let response = self
            .http
            .client()
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, referer)
            .header(reqwest::header::COOKIE, cookie)
            .timeout(Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
    }

    async fn get_text(&self, url: &str, cookie: &str, referer: &str) -> Result<String, AppError> {
        let response = self
            .http
            .client()
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, referer)
            .header(reqwest::header::COOKIE, cookie)
            .timeout(Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?;
        Ok(response.text().await?)
    }
}

/// 与 simple_live `_getWebCookie` 相同: HEAD 页面拿 ttwid/__ac_nonce/msToken。
pub(crate) async fn fresh_cookie(http: &HttpClient, url: &str) -> String {
    let response = http
        .client()
        .head(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, REFERER)
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    let mut cookie = String::new();
    if let Ok(resp) = response {
        for value in resp.headers().get_all(reqwest::header::SET_COOKIE).iter() {
            let Ok(text) = value.to_str() else { continue };
            let first = text.split(';').next().unwrap_or_default();
            if first.contains("ttwid") || first.contains("__ac_nonce") || first.contains("msToken") {
                cookie.push_str(first);
                cookie.push(';');
            }
        }
    }
    if cookie.is_empty() {
        DEFAULT_COOKIE.to_string()
    } else {
        cookie
    }
}

/// 拼接 a_bogus 签名后的完整 URL。rquickjs 非 Send, 放阻塞线程执行。
pub(crate) async fn abogus_url(url: &str) -> Result<String, AppError> {
    let url = url.to_string();
    tokio::task::spawn_blocking(move || run_abogus(&url))
        .await
        .map_err(|err| AppError::Internal(anyhow::anyhow!("签名线程失败: {err}")))?
        .map_err(|err| AppError::BadRequest(format!("抖音签名失败: {err}")))
}

fn run_abogus(url: &str) -> anyhow::Result<String> {
    let ms_token = generate_ms_token(107);
    let with_token = format!("{url}&msToken={ms_token}");
    let after = with_token.splitn(2, '?').nth(1).unwrap_or_default();
    // 与 Dart 一致: 余下部分若仍含 '?', 取第二个 '?' 之后。
    let query = match after.find('?') {
        Some(index) => &after[index + 1..],
        None => after,
    };
    let a_bogus: String = eval_js(
        ABOGUS_JS,
        &format!("getABogus({}, {})", js_str(query), js_str(USER_AGENT)),
    )?;
    Ok(format!(
        "{url}&msToken={}&a_bogus={}",
        urlencoding::encode(&ms_token),
        urlencoding::encode(&a_bogus)
    ))
}

/// 弹幕连接的 signature(webmssdk 的 X-Bogus)。
pub(crate) async fn danmaku_signature(room_id: &str, unique_id: &str) -> Result<String, AppError> {
    let room_id = room_id.to_string();
    let unique_id = unique_id.to_string();
    tokio::task::spawn_blocking(move || run_danmaku_signature(&room_id, &unique_id))
        .await
        .map_err(|err| AppError::Internal(anyhow::anyhow!("签名线程失败: {err}")))?
        .map_err(|err| AppError::BadRequest(format!("抖音弹幕签名失败: {err}")))
}

fn run_danmaku_signature(room_id: &str, unique_id: &str) -> anyhow::Result<String> {
    let stub_params = format!(
        "live_id=1,aid=6383,version_code=180800,webcast_sdk_version=1.3.0,room_id={room_id},sub_room_id=,sub_channel_id=,did_rule=3,user_unique_id={unique_id},device_platform=web,device_type=,ac=,identity=audience"
    );
    let digest = md5::Md5::digest(stub_params.as_bytes());
    let ms_stub = format!("{digest:x}");
    let call = format!(
        "getMSSDKSignature({}, {})",
        js_str(&ms_stub),
        js_str(USER_AGENT)
    );
    // 与 simple_live 一致: 签名里出现 '-' / '=' 时重算(WSS URL 里不安全)。
    let mut signature = String::new();
    for _ in 0..20 {
        signature = eval_js(WEBMSSDK_JS, &call)?;
        if !signature.contains('-') && !signature.contains('=') {
            break;
        }
    }
    Ok(signature)
}

fn eval_js(script: &str, call: &str) -> anyhow::Result<String> {
    use rquickjs::context::EvalOptions;
    let runtime = Runtime::new()?;
    runtime.set_max_stack_size(8 * 1024 * 1024);
    let context = Context::full(&runtime)?;
    context.with(|ctx| {
        // 混淆脚本依赖 sloppy 语义(如 `document = {}` 隐式全局), 必须关严格模式。
        let mut options = EvalOptions::default();
        options.strict = false;
        ctx.eval_with_options::<(), _>(script.as_bytes(), options)?;
        // 包一层 try/catch, 把 JS 异常信息带回 Rust(rquickjs 默认只给 "Exception")。
        let wrapped = format!(
            "(function(){{ try {{ return String(({})) }} catch (e) {{ return '__JS_ERR__:' + (e && e.message) + ' | ' + (e && e.stack) }} }})()",
            call
        );
        let mut options = EvalOptions::default();
        options.strict = false;
        let value: String = ctx.eval_with_options(wrapped.as_bytes(), options)?;
        if let Some(err) = value.strip_prefix("__JS_ERR__:") {
            return Err(anyhow::anyhow!("JS 执行异常: {err}"));
        }
        Ok(value)
    })
}

/// JSON 字符串字面量即合法 JS 字符串字面量, 免去手工转义。
fn js_str(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn generate_ms_token(length: usize) -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}

/// 弹幕用的 12 位随机数字 userUniqueId(simple_live: 非必需, 随机即可)。
pub(crate) fn generate_unique_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..12).map(|_| rng.gen_range(0..10).to_string()).collect()
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

/// `url_list` 数组的第一个地址。
fn first_url(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// 解析(纯函数, 便于单测)
// ---------------------------------------------------------------------------

/// 首页 HTML 里的转义 JSON: `{\"pathname\":\"/\",\"categoryData\":[...]`。
/// 数组以 `]}]}],` 收尾(没有转义换行), 只能用括号配对提取。
pub fn parse_categories(html: &str) -> Vec<LiveCategoryGroup> {
    let Some(array_src) = extract_escaped_array(html, r#"\"categoryData\":["#) else {
        return Vec::new();
    };
    let cleaned = array_src.replace("\\\"", "\"").replace("\\\\", "\\");
    let Ok(value) = serde_json::from_str::<Value>(&cleaned) else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let partition = item.get("partition")?;
            let id = format!("{},{}", text(partition, "id_str"), text(partition, "type"));
            let name = text(partition, "title");
            if name.is_empty() {
                return None;
            }
            let mut children = vec![LiveCategory {
                id: id.clone(),
                name: name.clone(),
                icon: String::new(),
            }];
            if let Some(subs) = item.get("sub_partition").and_then(Value::as_array) {
                for sub in subs {
                    let Some(p) = sub.get("partition") else { continue };
                    let sub_name = text(p, "title");
                    if sub_name.is_empty() {
                        continue;
                    }
                    children.push(LiveCategory {
                        id: format!("{},{}", text(p, "id_str"), text(p, "type")),
                        name: sub_name,
                        icon: String::new(),
                    });
                }
            }
            Some(LiveCategoryGroup { id, name, children })
        })
        .collect()
}

/// 在 pace_f 转义 JSON 里, 从 marker(以 `[` 结尾, 如 `\"categoryData\":[`)后的 `[` 开始,
/// 按括号配对提取整个转义数组字面量。`\"` 视为字符串边界, 串内括号不计数。
fn extract_escaped_array(html: &str, marker: &str) -> Option<String> {
    let start = html.find(marker)? + marker.len() - 1; // 指向 '['
    let bytes = html.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'"' {
                in_string = !in_string;
            }
            i += 2; // 转义序列成对跳过
            continue;
        }
        if !in_string {
            match b {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(html[start..=i].to_string());
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// 分区房间列表: `data.data[]`。
pub fn parse_partition_rooms(value: &Value) -> Vec<LiveRoomSummary> {
    let Some(items) = value.pointer("/data/data").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let room_id = text(item, "web_rid");
            let room = item.get("room")?;
            if room_id.is_empty() {
                return None;
            }
            Some(LiveRoomSummary {
                platform: PLATFORM_ID.to_string(),
                room_id,
                title: text(room, "title"),
                cover: first_url(room.pointer("/cover/url_list")),
                user_name: text(room.get("owner")?, "nickname"),
                online: number(room.pointer("/room_view_stats")?, "display_value"),
                area_name: String::new(),
            })
        })
        .collect()
}

/// 搜索结果: `data[].lives.rawdata` 是内嵌 JSON 字符串。
pub fn parse_search(value: &Value) -> Vec<LiveRoomSummary> {
    let Some(items) = value.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let raw = item.pointer("/lives/rawdata")?;
            let data = match raw {
                Value::String(s) => serde_json::from_str::<Value>(s).ok()?,
                other => other.clone(),
            };
            let owner = data.get("owner")?;
            let room_id = text(owner, "web_rid");
            if room_id.is_empty() {
                return None;
            }
            Some(LiveRoomSummary {
                platform: PLATFORM_ID.to_string(),
                room_id,
                title: text(&data, "title"),
                cover: first_url(data.pointer("/cover/url_list")),
                user_name: text(owner, "nickname"),
                online: number(data.get("stats")?, "total_user"),
                area_name: String::new(),
            })
        })
        .collect()
}

/// enter API: `data.data[0]` 是房间, `data.user` 是主播(未开播时兜昵称/头像)。
pub fn parse_enter_detail(data: &Value, web_rid: &str) -> Option<LiveRoomDetail> {
    let room = data.get("data")?.as_array()?.first()?;
    let user = data.get("user").cloned().unwrap_or(Value::Null);
    let owner = room.get("owner").cloned().unwrap_or(Value::Null);
    let is_live = room.get("status").and_then(Value::as_i64) == Some(2);
    let (user_name, avatar) = if is_live {
        (
            text(&owner, "nickname"),
            first_url(owner.pointer("/avatar_thumb/url_list")),
        )
    } else {
        (
            text(&user, "nickname"),
            first_url(user.pointer("/avatar_thumb/url_list")),
        )
    };
    Some(LiveRoomDetail {
        platform: PLATFORM_ID.to_string(),
        room_id: web_rid.to_string(),
        title: text(room, "title"),
        cover: if is_live {
            first_url(room.pointer("/cover/url_list"))
        } else {
            String::new()
        },
        user_name,
        avatar,
        online: if is_live {
            number(room.pointer("/room_view_stats")?, "display_value")
        } else {
            0
        },
        followers: 0,
        description: text(&owner, "signature"),
        area_name: String::new(),
        parent_area_name: String::new(),
        is_live,
        // 抖音公开接口未提供开播时间。
        live_start_time: 0,
    })
}

/// reflow API: `data.room`。
pub fn parse_reflow_detail(value: &Value, room_id: &str) -> Option<LiveRoomDetail> {
    let room = value.pointer("/data/room")?;
    let owner = room.get("owner").cloned().unwrap_or(Value::Null);
    let is_live = room.get("status").and_then(Value::as_i64) == Some(2);
    let web_rid = text(&owner, "web_rid");
    Some(LiveRoomDetail {
        platform: PLATFORM_ID.to_string(),
        room_id: if web_rid.is_empty() {
            room_id.to_string()
        } else {
            web_rid
        },
        title: text(room, "title"),
        cover: if is_live {
            first_url(room.pointer("/cover/url_list"))
        } else {
            String::new()
        },
        user_name: text(&owner, "nickname"),
        avatar: first_url(owner.pointer("/avatar_thumb/url_list")),
        online: if is_live {
            number(room.pointer("/room_view_stats")?, "display_value")
        } else {
            0
        },
        followers: 0,
        description: text(&owner, "signature"),
        area_name: String::new(),
        parent_area_name: String::new(),
        is_live,
        // 抖音公开接口未提供开播时间。
        live_start_time: 0,
    })
}

/// 房间页 HTML 里的 `state` 节点。
pub fn parse_html_state(html: &str) -> Option<Value> {
    let re = Regex::new(r#"\{\\"state\\":\{\\"appStore.*?\]\\n"#).expect("regex");
    let matched = re.find(html)?;
    let cleaned = matched
        .as_str()
        .trim()
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
        .replace("]\\n", "");
    let value = serde_json::from_str::<Value>(&cleaned).ok()?;
    value.get("state").cloned()
}

/// HTML 后备路径: `state.roomStore.roomInfo.{room,anchor}`。
pub fn parse_html_detail(state: &Value, web_rid: &str) -> Option<LiveRoomDetail> {
    let room = state.pointer("/roomStore/roomInfo/room")?;
    let anchor = state.pointer("/roomStore/roomInfo/anchor").cloned().unwrap_or(Value::Null);
    let owner = room.get("owner").cloned().unwrap_or(Value::Null);
    let is_live = room.get("status").and_then(Value::as_i64) == Some(2);
    let (user_name, avatar) = if is_live {
        (
            text(&owner, "nickname"),
            first_url(owner.pointer("/avatar_thumb/url_list")),
        )
    } else {
        (
            text(&anchor, "nickname"),
            first_url(anchor.pointer("/avatar_thumb/url_list")),
        )
    };
    Some(LiveRoomDetail {
        platform: PLATFORM_ID.to_string(),
        room_id: web_rid.to_string(),
        title: text(room, "title"),
        cover: if is_live {
            first_url(room.pointer("/cover/url_list"))
        } else {
            String::new()
        },
        user_name,
        avatar,
        online: if is_live {
            number(room.pointer("/room_view_stats")?, "display_value")
        } else {
            0
        },
        followers: 0,
        description: text(&owner, "signature"),
        area_name: String::new(),
        parent_area_name: String::new(),
        is_live,
        // 抖音公开接口未提供开播时间。
        live_start_time: 0,
    })
}

/// 从 `stream_url` 解析各画质地址。`stream_data` 是内嵌 JSON 时按 sdk_key 取;
/// 否则用 `flv_pull_url` / `hls_pull_url_map` 的「长度 - level」索引(与 simple_live 一致)。
pub fn parse_streams(stream_url: &Value) -> Vec<LiveStream> {
    let Some(pull) = stream_url.pointer("/live_core_sdk_data/pull_data") else {
        return Vec::new();
    };
    let Some(qualities) = pull.pointer("/options/qualities").and_then(Value::as_array) else {
        return Vec::new();
    };
    let stream_data = pull.get("stream_data").and_then(Value::as_str).unwrap_or_default();
    let mut entries: Vec<(i64, String, String, String)> = Vec::new(); // (level, quality, format, url)

    if stream_data.starts_with('{') {
        let data = serde_json::from_str::<Value>(stream_data)
            .ok()
            .and_then(|v| v.get("data").cloned())
            .unwrap_or(Value::Null);
        for quality in qualities {
            let level = quality.get("level").and_then(Value::as_i64).unwrap_or(0);
            let name = text(quality, "name");
            let sdk_key = text(quality, "sdk_key");
            let flv = data
                .pointer(&format!("/{sdk_key}/main/flv"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let hls = data
                .pointer(&format!("/{sdk_key}/main/hls"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            push_stream(&mut entries, level, &name, "hls", hls);
            push_stream(&mut entries, level, &name, "flv", flv);
        }
    } else {
        let flv_list = map_values(stream_url.get("flv_pull_url"));
        let hls_list = map_values(stream_url.get("hls_pull_url_map"));
        for quality in qualities {
            let level = quality.get("level").and_then(Value::as_i64).unwrap_or(0);
            let name = text(quality, "name");
            let pick = |list: &[String]| -> String {
                let index = list.len() as i64 - level;
                if (0..list.len() as i64).contains(&index) {
                    list[index as usize].clone()
                } else {
                    String::new()
                }
            };
            push_stream(&mut entries, level, &name, "hls", &pick(&hls_list));
            push_stream(&mut entries, level, &name, "flv", &pick(&flv_list));
        }
    }

    // 画质从高到低; 同画质 HLS 在前(WebView 优先走 hls.js)。
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter(|(_, _, _, url)| seen.insert(url.clone()))
        .map(|(_, quality, format, url)| LiveStream {
            format,
            quality,
            url,
        })
        .collect()
}

fn push_stream(
    entries: &mut Vec<(i64, String, String, String)>,
    level: i64,
    quality: &str,
    format: &str,
    url: &str,
) {
    if url.is_empty() || quality.is_empty() {
        return;
    }
    entries.push((level, quality.to_string(), format.to_string(), url.to_string()));
}

fn map_values(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.values()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_categories_from_escaped_render_data() {
        // 真实页面: 转义 JSON 嵌在 children 数组里, categoryData 以 `]}]}],` 收尾。
        let html = r#"self.__pace_f.push([1,"{\"children\":[[\"$\",\"$La\",null,{\"pathname\":\"/\",\"categoryData\":[{\"partition\":{\"id_str\":\"720\",\"type\":1,\"title\":\"推荐\"},\"sub_partition\":[{\"partition\":{\"id_str\":\"844\",\"type\":1,\"title\":\"王者荣耀\"}}]},{\"partition\":{\"id_str\":\"101\",\"type\":4,\"title\":\"聊天\"},\"sub_partition\":[]}]}],\"$undefined\"]}]"])"#;
        let groups = parse_categories(html);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "720,1");
        assert_eq!(groups[0].name, "推荐");
        assert_eq!(groups[0].children.len(), 2);
        assert_eq!(groups[0].children[0].id, "720,1");
        assert_eq!(groups[0].children[1].id, "844,1");
        assert_eq!(groups[0].children[1].name, "王者荣耀");
        assert_eq!(groups[1].id, "101,4");
        assert_eq!(groups[1].name, "聊天");
        assert_eq!(groups[1].children.len(), 1);
    }

    #[test]
    fn parses_partition_rooms() {
        let value = serde_json::json!({
            "data": { "data": [
                { "web_rid": "416144012050",
                  "room": { "title": "上分之路",
                            "cover": { "url_list": ["https://cover/a.jpg"] },
                            "owner": { "nickname": "主播甲" },
                            "room_view_stats": { "display_value": 1234 } } },
                { "web_rid": "", "room": { "title": "坏数据" } }
            ]}
        });
        let rooms = parse_partition_rooms(&value);
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "416144012050");
        assert_eq!(rooms[0].title, "上分之路");
        assert_eq!(rooms[0].cover, "https://cover/a.jpg");
        assert_eq!(rooms[0].online, 1234);
    }

    #[test]
    fn parses_search_rawdata_string() {
        let raw = serde_json::json!({
            "owner": { "web_rid": "416144012050", "nickname": "主播乙" },
            "title": "聊天",
            "cover": { "url_list": ["https://cover/b.jpg"] },
            "stats": { "total_user": "567" }
        })
        .to_string();
        let value = serde_json::json!({ "data": [ { "lives": { "rawdata": raw } } ] });
        let rooms = parse_search(&value);
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "416144012050");
        assert_eq!(rooms[0].user_name, "主播乙");
        assert_eq!(rooms[0].online, 567);
    }

    #[test]
    fn parses_enter_detail_live_and_offline() {
        let live = serde_json::json!({
            "data": [ { "status": 2, "title": "直播中",
                        "cover": { "url_list": ["https://cover/c.jpg"] },
                        "owner": { "nickname": "主播", "signature": "你好",
                                   "avatar_thumb": { "url_list": ["https://avatar/a.jpg"] } },
                        "room_view_stats": { "display_value": 99 } } ],
            "user": { "nickname": "主播" }
        });
        let detail = parse_enter_detail(&live, "416144012050").expect("detail");
        assert!(detail.is_live);
        assert_eq!(detail.online, 99);
        assert_eq!(detail.cover, "https://cover/c.jpg");
        assert_eq!(detail.description, "你好");

        let offline = serde_json::json!({
            "data": [ { "status": 4, "title": "下播了",
                        "owner": { "nickname": "主播" } } ],
            "user": { "nickname": "主播", "avatar_thumb": { "url_list": ["https://avatar/a.jpg"] } }
        });
        let detail = parse_enter_detail(&offline, "416144012050").expect("detail");
        assert!(!detail.is_live);
        assert_eq!(detail.online, 0);
        assert_eq!(detail.cover, "");
        assert_eq!(detail.avatar, "https://avatar/a.jpg");
    }

    #[test]
    fn parses_streams_from_embedded_stream_data() {
        let stream_data = serde_json::json!({
            "data": {
                "ao": { "main": { "flv": "https://pull-flv/a.flv", "hls": "https://pull-hls/a.m3u8" } },
                "sd": { "main": { "flv": "https://pull-flv/b.flv", "hls": "" } }
            }
        })
        .to_string();
        let stream_url = serde_json::json!({
            "live_core_sdk_data": { "pull_data": {
                "options": { "qualities": [
                    { "name": "蓝光", "level": 5, "sdk_key": "ao" },
                    { "name": "高清", "level": 2, "sdk_key": "sd" }
                ]},
                "stream_data": stream_data
            }}
        });
        let streams = parse_streams(&stream_url);
        assert_eq!(streams.len(), 3);
        assert_eq!(streams[0].quality, "蓝光");
        assert_eq!(streams[0].format, "hls");
        assert_eq!(streams[0].url, "https://pull-hls/a.m3u8");
        assert_eq!(streams[1].format, "flv");
        assert_eq!(streams[2].quality, "高清");
        assert_eq!(streams[2].format, "flv");
    }

    #[test]
    fn parses_streams_from_pull_url_maps() {
        let stream_url = serde_json::json!({
            "live_core_sdk_data": { "pull_data": {
                "options": { "qualities": [
                    { "name": "原画", "level": 2, "sdk_key": "origin" },
                    { "name": "高清", "level": 1, "sdk_key": "hd" }
                ]},
                "stream_data": ""
            }},
            "flv_pull_url": { "HD": "https://pull-flv/hd.flv", "SD": "https://pull-flv/sd.flv" },
            "hls_pull_url_map": { "HD": "https://pull-hls/hd.m3u8", "SD": "https://pull-hls/sd.m3u8" }
        });
        let streams = parse_streams(&stream_url);
        // 原画 level=2 → index 0(HD 位), 高清 level=1 → index 1。
        assert_eq!(streams.len(), 4);
        assert_eq!(streams[0].url, "https://pull-hls/hd.m3u8");
        assert_eq!(streams[1].url, "https://pull-flv/hd.flv");
        assert_eq!(streams[2].url, "https://pull-hls/sd.m3u8");
    }

    #[test]
    fn ms_token_has_expected_charset() {
        let token = generate_ms_token(107);
        assert_eq!(token.len(), 107);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn unique_id_is_12_digits() {
        let id = generate_unique_id();
        assert_eq!(id.len(), 12);
        assert!(id.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn abogus_js_runs_and_signs() {
        // 不依赖网络: 只验证混淆 JS 能在 rquickjs 里跑通并产出签名。
        let signed = run_abogus("https://live.douyin.com/webcast/room/web/enter/?aid=6383&web_rid=123&msToken=")
            .expect("sign");
        assert!(signed.contains("&msToken="));
        assert!(signed.contains("&a_bogus="));
        let a_bogus = signed.rsplit("&a_bogus=").next().unwrap_or_default();
        assert!(!a_bogus.is_empty());
    }

    #[test]
    fn webmssdk_js_runs_and_signs() {
        let signature = run_danmaku_signature("7376429659866598196", "123456789012")
            .expect("signature");
        assert!(!signature.is_empty());
        assert!(!signature.contains('-'));
        assert!(!signature.contains('='));
    }

    #[tokio::test]
    #[ignore]
    async fn fetches_real_categories_and_rooms() {
        let service = Douyin::new(HttpClient::new(15, None).expect("http"));
        let groups = service.categories().await.expect("categories");
        assert!(!groups.is_empty());
        println!("分区数: {}", groups.len());
        for group in groups.iter().take(3) {
            println!("  {}({}) 子分区 {}", group.name, group.id, group.children.len());
        }
        let rooms = service.rooms("", "", 1).await.expect("recommend rooms");
        assert!(!rooms.is_empty());
        println!("推荐房间数: {}", rooms.len());
        let first = &rooms[0];
        println!("第一个房间: {} {} {} 在线 {}", first.room_id, first.title, first.user_name, first.online);

        let detail = service.room_detail(&first.room_id).await.expect("detail");
        println!("详情: {} 直播={} 在线 {}", detail.title, detail.is_live, detail.online);
        let streams = service.stream_urls(&first.room_id).await.expect("streams");
        println!("在播={} 线路数 {}", streams.is_live, streams.streams.len());
        for s in streams.streams.iter().take(4) {
            println!("  {} {} {}", s.quality, s.format, &s.url[..s.url.len().min(80)]);
        }
        assert!(streams.is_live);
        assert!(!streams.streams.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn searches_real_rooms() {
        let service = Douyin::new(HttpClient::new(15, None).expect("http"));
        // 2026 起抖音搜索必须登录: 匿名时要求返回可读错误, 有结果时则必须非空。
        match service.search("王者荣耀", 1).await {
            Ok(rooms) => {
                println!("搜索结果: {}", rooms.len());
                for room in rooms.iter().take(5) {
                    println!("  {} {} {} 在线 {}", room.room_id, room.title, room.user_name, room.online);
                }
                assert!(!rooms.is_empty());
            }
            Err(err) => {
                println!("搜索不可用(预期): {err}");
                assert!(err.to_string().contains("抖音搜索不可用"), "意外错误: {err}");
            }
        }
    }
}
