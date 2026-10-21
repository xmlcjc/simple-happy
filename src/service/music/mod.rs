//! 音乐功能服务。
//!
//! 职责边界：
//! - `plugin`：MusicFree 插件的安装、元信息与 JS 运行时；
//! - `library` / `local`：收藏、歌单、历史、本地曲库（后续阶段接入）；
//! - 本文件只做聚合与平台无关的接口, 不直接写 SQL、不碰 JS 细节。
//!
//! 隔离约定：音乐模块只依赖 `crawler::http_client::HttpClient`、
//! `storage::db` 与 `error`，不得引用阅读/直播模块的内部实现。

pub mod library;
pub mod local;
pub mod model;
pub mod plugin;
pub mod proxy;

use std::time::Duration;

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use self::library::MusicLibrary;
use self::local::MusicLocalLibrary;
use self::model::{
    MusicItem, MusicLyric, MusicMediaSource, MusicPlugin, MusicSearchPage,
};
use self::plugin::PluginRegistry;

/// 插件调用超时。搜索聚合的首次请求可能较慢, 给到 20s。
const SEARCH_TIMEOUT: Duration = Duration::from_secs(20);
const MEDIA_TIMEOUT: Duration = Duration::from_secs(20);
const LYRIC_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct MusicService {
    pool: SqlitePool,
    http: HttpClient,
    plugins: PluginRegistry,
    library: MusicLibrary,
    local: MusicLocalLibrary,
}

impl MusicService {
    pub fn new(pool: SqlitePool, http: HttpClient, storage_dir: &str) -> Self {
        let plugins = PluginRegistry::new(pool.clone(), storage_dir, http.clone());
        let library = MusicLibrary::new(pool.clone());
        let local = MusicLocalLibrary::new(pool.clone());
        Self {
            pool,
            http,
            plugins,
            library,
            local,
        }
    }

    pub fn plugins(&self) -> &PluginRegistry {
        &self.plugins
    }

    /// 收藏 / 歌单 / 历史 / 统计。
    pub fn library(&self) -> &MusicLibrary {
        &self.library
    }

    /// 本地音乐曲库。
    pub fn local(&self) -> &MusicLocalLibrary {
        &self.local
    }

    /// 预留给曲库服务与本地导入（P3/P4）。
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// 预留给音频代理取流（P2）。
    pub fn http(&self) -> HttpClient {
        self.http.clone()
    }

    pub async fn list_plugins(&self) -> Result<Vec<MusicPlugin>, AppError> {
        self.plugins.list().await
    }

    pub async fn install_plugin(
        &self,
        source: &str,
        file_name: Option<&str>,
    ) -> Result<MusicPlugin, AppError> {
        self.plugins.install(source, file_name).await
    }

    pub async fn uninstall_plugin(&self, platform: &str) -> Result<bool, AppError> {
        self.plugins.uninstall(platform).await
    }

    pub async fn set_plugin_enabled(
        &self,
        platform: &str,
        enabled: bool,
    ) -> Result<bool, AppError> {
        self.plugins.set_enabled(platform, enabled).await
    }

    pub async fn search(
        &self,
        platform: &str,
        keyword: &str,
        page: u32,
        search_type: &str,
    ) -> Result<MusicSearchPage, AppError> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(AppError::BadRequest("搜索关键词不能为空".to_string()));
        }
        let plugin = self.require_plugin(platform).await?;
        let value = self
            .plugins
            .call(
                &plugin,
                "search",
                json!([keyword, page.max(1), search_type]),
                SEARCH_TIMEOUT,
            )
            .await?;
        Ok(build_search_page(&plugin.platform, &value))
    }

    pub async fn media_source(
        &self,
        platform: &str,
        item: &MusicItem,
        quality: &str,
    ) -> Result<MusicMediaSource, AppError> {
        let plugin = self.require_plugin(platform).await?;
        let item_value = serde_json::to_value(item)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("无法序列化歌曲: {e}")))?;
        let value = self
            .plugins
            .call(
                &plugin,
                "getMediaSource",
                json!([item_value, quality]),
                MEDIA_TIMEOUT,
            )
            .await?;
        build_media_source(&value)
    }

    pub async fn lyric(
        &self,
        platform: &str,
        item: &MusicItem,
    ) -> Result<MusicLyric, AppError> {
        let plugin = self.require_plugin(platform).await?;
        let item_value = serde_json::to_value(item)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("无法序列化歌曲: {e}")))?;
        let value = self
            .plugins
            .call(&plugin, "getLyric", json!([item_value]), LYRIC_TIMEOUT)
            .await?;
        Ok(build_lyric(&value))
    }

    /// 带自定义头取回整块音频字节, 供 `reader://musicStream` 使用。
    pub async fn stream(
        &self,
        url: &str,
        headers: &serde_json::Map<String, Value>,
    ) -> Result<proxy::AudioStream, AppError> {
        proxy::fetch_audio(&self.http, url, headers).await
    }

    async fn require_plugin(&self, platform: &str) -> Result<MusicPlugin, AppError> {
        let platform = platform.trim();
        if platform.is_empty() {
            return Err(AppError::BadRequest("请先选择一个音乐插件".to_string()));
        }
        let plugin = self
            .plugins
            .get(platform)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("插件 {platform} 未安装")))?;
        if !plugin.enabled {
            return Err(AppError::BadRequest(format!("插件 {} 已停用", plugin.name)));
        }
        Ok(plugin)
    }
}

/// 把插件 search 的返回值归一成结果页。
///
/// 兼容两种形态：`{ data: [...], isEnd }` 与直接返回数组（部分老插件）。
fn build_search_page(platform: &str, value: &Value) -> MusicSearchPage {
    let data = value.get("data").unwrap_or(value);
    let items = data
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|raw| normalize_item(platform, raw))
                .collect()
        })
        .unwrap_or_default();
    let is_end = value.get("isEnd").and_then(Value::as_bool).unwrap_or(true);
    MusicSearchPage {
        items,
        is_end,
        platform: platform.to_string(),
    }
}

fn normalize_item(platform: &str, raw: &Value) -> Option<MusicItem> {
    if !raw.is_object() {
        return None;
    }
    let mut item: MusicItem = serde_json::from_value(raw.clone()).ok()?;
    if item.platform.is_empty() {
        item.platform = platform.to_string();
    }
    if item.id.is_empty() {
        return None;
    }
    Some(item)
}

/// 兼容 `{url, headers}`、纯字符串与 `{url: ...}` 三种返回。
fn build_media_source(value: &Value) -> Result<MusicMediaSource, AppError> {
    if let Some(url) = value.as_str() {
        if url.trim().is_empty() {
            return Err(AppError::BadRequest("插件未返回播放地址".to_string()));
        }
        return Ok(MusicMediaSource {
            url: url.to_string(),
            quality: "standard".to_string(),
            headers: Default::default(),
        });
    }
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if url.is_empty() {
        return Err(AppError::BadRequest(
            "插件未返回可用的播放地址（可能需要登录或该歌曲无版权）".to_string(),
        ));
    }
    let headers = value
        .get("headers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    Ok(MusicMediaSource {
        url: url.to_string(),
        quality: value
            .get("quality")
            .and_then(Value::as_str)
            .unwrap_or("standard")
            .to_string(),
        headers,
    })
}

/// MusicFree 的 getLyric 返回 LRC 文本；少数插件返回对象。
fn build_lyric(value: &Value) -> MusicLyric {
    if let Some(raw) = value.as_str() {
        return MusicLyric {
            raw: raw.to_string(),
            translation: String::new(),
        };
    }
    MusicLyric {
        raw: value
            .get("raw")
            .or_else(|| value.get("lrc"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        translation: value
            .get("translation")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_search_results_and_fills_platform() {
        let value = json!({
            "isEnd": false,
            "data": [
                { "id": "1", "title": "A", "artist": "B" },
                { "platform": "other", "id": "2", "title": "C" },
                { "title": "missing id" }
            ]
        });
        let page = build_search_page("demo", &value);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].platform, "demo");
        assert_eq!(page.items[1].platform, "other");
        assert!(!page.is_end);
    }

    #[test]
    fn accepts_plain_array_search_results() {
        let page = build_search_page("demo", &json!([{ "id": "1", "title": "A" }]));
        assert_eq!(page.items.len(), 1);
        assert!(page.is_end);
    }

    #[test]
    fn parses_media_source_forms() {
        let plain = build_media_source(&json!("https://a/b.mp3")).unwrap();
        assert_eq!(plain.url, "https://a/b.mp3");

        let with_headers = build_media_source(&json!({
            "url": "https://a/b.mp3",
            "headers": { "Referer": "https://a" }
        }))
        .unwrap();
        assert_eq!(with_headers.headers.get("Referer").unwrap(), "https://a");

        assert!(build_media_source(&json!({})).is_err());
    }

    #[test]
    fn parses_lyric_forms() {
        assert_eq!(build_lyric(&json!("[00:00]hi")).raw, "[00:00]hi");
        let object = build_lyric(&json!({ "raw": "raw", "translation": "tr" }));
        assert_eq!(object.raw, "raw");
        assert_eq!(object.translation, "tr");
    }
}
