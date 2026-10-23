//! 直播 HLS 本地代理。
//!
//! 虎牙的 HLS 地址带 `wsSecret` 签名, CDN 回源时会**间歇性返回 403**
//! (同一地址实测 8 次请求 6 次 403、2 次 200, 几秒后又恢复)。浏览器里的
//! hls.js 对 4xx 不做重试, 直接把错误抛给播放器, 于是前端每隔几秒就要
//! "重连中 1/3"。
//!
//! 这里把清单与分片都收进 Rust: 带 UA/Referer 请求上游、失败自动重试、
//! 换 CDN 线路, 必要时重新调平台接口拿一份新签名; 实在拿不到时回退到最近
//! 一次可用的清单, 让播放器保持存活而不是报致命错误。
//!
//! 前端只需把流地址换成 `${readerOrigin}/liveHls?...`。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use url::Url;

use crate::error::error::AppError;

use super::LiveService;

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const REFERER: &str = "https://www.huya.com/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
/// 线路列表缓存时长: 同一房间短时间内重复刷新清单不必反复调平台接口。
const LINES_TTL: Duration = Duration::from_secs(20);
/// 全部线路都失败时, 允许回退到多久以内的旧清单。
const STALE_MANIFEST_TTL: Duration = Duration::from_secs(90);

#[derive(Clone)]
struct CachedRoom {
    /// 该房间可用的 CDN 线路(按优先级)。
    lines: Vec<String>,
    /// 最近一次成功拉到的原始清单。
    manifest: String,
    /// 清单里相对分片地址的基准。
    manifest_url: String,
    updated_at: Instant,
}

static CACHE: Lazy<Mutex<HashMap<String, CachedRoom>>> = Lazy::new(|| Mutex::new(HashMap::new()));

impl LiveService {
    /// 代理 HLS 清单: 取上游 m3u8 并把分片地址改写成指向本代理。
    ///
    /// `preferred` 是前端手里正在播的那条地址, 优先用它; 失败后再换其它线路。
    pub async fn hls_playlist(
        &self,
        platform: &str,
        room_id: &str,
        qn: Option<i64>,
        preferred: Option<&str>,
    ) -> Result<String, AppError> {
        let key = format!("{platform}:{room_id}:{}", qn.unwrap_or(0));
        let mut lines = self.cached_lines(&key);
        if let Some(url) = preferred.map(str::trim).filter(|value| !value.is_empty()) {
            if !lines.iter().any(|line| line == url) {
                lines.insert(0, url.to_string());
            }
        }

        let mut last_error = String::new();
        for round in 0..2 {
            if lines.is_empty() {
                lines = self.hls_candidates(platform, room_id, qn).await;
            }
            for line in &lines {
                // 同一地址也可能被随机 403, 每条线路再重试一次; 多线路叠加后
                // 一次刷新拿到清单的概率足够高。
                for attempt in 0..2 {
                    match self.fetch_text(line).await {
                        Ok(Some(text)) if text.trim_start().starts_with("#EXTM3U") => {
                            self.remember(&key, lines.clone(), text.clone(), line.clone());
                            return Ok(rewrite_manifest(&text, line, platform, room_id, qn));
                        }
                        Ok(_) => last_error = format!("清单不可用: {line}"),
                        Err(err) => last_error = err,
                    }
                    if attempt == 0 {
                        tokio::time::sleep(Duration::from_millis(120)).await;
                    }
                }
            }
            if round == 0 {
                // 线路可能整体 403(签名过期/节点异常), 重新向平台要一份。
                let fresh = self.hls_candidates(platform, room_id, qn).await;
                if !fresh.is_empty() {
                    lines = fresh;
                } else {
                    break;
                }
            }
        }

        // 上游全部失败: 用最近一次的清单续命, 让 hls.js 继续重试分片而不是 fatal。
        if let Some(cached) = self.cached_room(&key) {
            if cached.updated_at.elapsed() < STALE_MANIFEST_TTL && !cached.manifest.is_empty() {
                return Ok(rewrite_manifest(
                    &cached.manifest,
                    &cached.manifest_url,
                    platform,
                    room_id,
                    qn,
                ));
            }
        }
        Err(AppError::Internal(anyhow::anyhow!(
            "直播流清单获取失败: {last_error}"
        )))
    }

    /// 代理 HLS 分片: 上游 403/超时自动重试数次。
    pub async fn hls_segment(&self, url: &str) -> Result<(Vec<u8>, String), AppError> {
        let mut last_error = String::new();
        for attempt in 0..5u32 {
            match self.fetch_bytes(url).await {
                Ok(Some((bytes, content_type))) => return Ok((bytes, content_type)),
                Ok(None) => last_error = "上游拒绝访问(403/404)".to_string(),
                Err(err) => last_error = err,
            }
            if attempt < 4 {
                tokio::time::sleep(Duration::from_millis(120 * u64::from(attempt + 1))).await;
            }
        }
        Err(AppError::Internal(anyhow::anyhow!(
            "直播分片获取失败: {last_error}"
        )))
    }

    /// 向平台要一组候选线路。目前只有虎牙需要走代理。
    async fn hls_candidates(
        &self,
        platform: &str,
        room_id: &str,
        qn: Option<i64>,
    ) -> Vec<String> {
        if platform != super::huya::PLATFORM_ID {
            return Vec::new();
        }
        match self.huya.stream_urls(room_id, qn).await {
            Ok(info) => info.streams.into_iter().map(|item| item.url).collect(),
            Err(_) => Vec::new(),
        }
    }

    async fn fetch_text(&self, url: &str) -> Result<Option<String>, String> {
        let response = self
            .http()
            .client()
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, REFERER)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|err| err.to_string())?;
        if !response.status().is_success() {
            return Ok(None);
        }
        response.text().await.map(Some).map_err(|err| err.to_string())
    }

    async fn fetch_bytes(&self, url: &str) -> Result<Option<(Vec<u8>, String)>, String> {
        let response = self
            .http()
            .client()
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, REFERER)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|err| err.to_string())?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .unwrap_or_else(|| "video/mp2t".to_string());
        let bytes = response.bytes().await.map_err(|err| err.to_string())?;
        Ok(Some((bytes.to_vec(), content_type)))
    }

    fn cached_room(&self, key: &str) -> Option<CachedRoom> {
        CACHE
            .lock()
            .ok()
            .and_then(|cache| cache.get(key).cloned())
    }

    fn cached_lines(&self, key: &str) -> Vec<String> {
        match self.cached_room(key) {
            Some(room) if room.updated_at.elapsed() < LINES_TTL => room.lines,
            _ => Vec::new(),
        }
    }

    fn remember(&self, key: &str, lines: Vec<String>, manifest: String, manifest_url: String) {
        if let Ok(mut cache) = CACHE.lock() {
            cache.insert(
                key.to_string(),
                CachedRoom {
                    lines,
                    manifest,
                    manifest_url,
                    updated_at: Instant::now(),
                },
            );
        }
    }
}

/// 把 m3u8 里的分片地址换成 `/liveHls?...&seg=<绝对地址>`。
fn rewrite_manifest(
    raw: &str,
    manifest_url: &str,
    platform: &str,
    room_id: &str,
    qn: Option<i64>,
) -> String {
    let origin = crate::api::protocol::reader_scheme_origin();
    let base = Url::parse(manifest_url).ok();
    let mut output = String::with_capacity(raw.len() + 1024);
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            output.push('\n');
            continue;
        }
        if trimmed.starts_with('#') {
            output.push_str(trimmed);
            output.push('\n');
            continue;
        }
        let absolute = base
            .as_ref()
            .and_then(|value| value.join(trimmed).ok())
            .map(|value| value.to_string())
            .unwrap_or_else(|| trimmed.to_string());
        output.push_str(&format!(
            "{origin}/liveHls?platform={}&roomId={}&qn={}&seg={}\n",
            urlencoding::encode(platform),
            urlencoding::encode(room_id),
            qn.unwrap_or(0),
            urlencoding::encode(&absolute),
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::rewrite_manifest;

    #[test]
    fn rewrites_segments_to_local_proxy() {
        let raw = "#EXTM3U\n#EXT-X-TARGETDURATION:4\nseg-1.ts\n/sub/seg-2.ts?x=1&y=2\n";
        let output = rewrite_manifest(
            raw,
            "http://hs.hls.huya.com/src/a.m3u8?ratio=2000",
            "huya",
            "991108",
            Some(2000),
        );
        assert!(output.contains("#EXTM3U"));
        assert!(!output.contains("\nseg-1.ts"));
        assert!(output.contains("/liveHls?platform=huya&roomId=991108&qn=2000&seg="));
        // 相对地址已补成绝对并 URL 编码(冒号/斜杠被编码, 且不残留未转义的 &)。
        assert!(output.contains("http%3A%2F%2Fhs.hls.huya.com"));
        assert!(output.contains("seg-2.ts%3Fx%3D1%26y%3D2"));
    }

    /// 真实网络冒烟测试: `cargo test --lib hls_proxy -- --ignored`。
    /// 默认忽略, 避免离线环境/CI 依赖虎牙可用性。
    #[tokio::test]
    #[ignore]
    async fn proxies_real_huya_playlist_and_segment() {
        use crate::crawler::http_client::HttpClient;
        let http = HttpClient::new(15, None).expect("http client");
        let service = super::LiveService::new(http);
        let playlist = service
            .hls_playlist("huya", "991108", None, None)
            .await
            .expect("playlist");
        assert!(playlist.contains("/liveHls?"));
        let segment = playlist
            .lines()
            .find(|line| !line.starts_with('#'))
            .expect("segment line");
        let encoded = segment
            .split("seg=")
            .nth(1)
            .expect("seg param");
        let url = urlencoding::decode(encoded).expect("decode").into_owned();
        let (bytes, _) = service.hls_segment(&url).await.expect("segment bytes");
        assert!(bytes.len() > 10_000, "segment too small: {}", bytes.len());
    }
}
