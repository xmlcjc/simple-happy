//! 音乐功能服务。
//!
//! 职责边界：
//! - `plugin`：MusicFree 插件的安装、元信息与 JS 运行时；
//! - `library` / `local`：收藏、歌单、历史、本地曲库（后续阶段接入）；
//! - 本文件只做聚合与平台无关的接口, 不直接写 SQL、不碰 JS 细节。
//!
//! 隔离约定：音乐模块只依赖 `crawler::http_client::HttpClient`、
//! `storage::db` 与 `error`，不得引用阅读/直播模块的内部实现。

pub mod cache;
pub mod library;
pub mod local;
pub mod model;
pub mod plugin;
pub mod proxy;

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use self::cache::MusicCache;
use self::library::MusicLibrary;
use self::local::MusicLocalLibrary;
use self::model::{
    MusicCollectionDetail, MusicCollectionItem, MusicCollectionPage, MusicItem, MusicLyric,
    MusicMediaSource, MusicPlugin, MusicRecommendTags, MusicSearchPage, MusicSheetGroup, MusicTag,
    MusicTagGroup, PluginInstallResult, PluginSubscription, PluginVariable,
};
use self::plugin::PluginRegistry;

/// 插件调用超时。搜索聚合的首次请求可能较慢, 给到 20s。
const SEARCH_TIMEOUT: Duration = Duration::from_secs(20);
const MEDIA_TIMEOUT: Duration = Duration::from_secs(20);
const LYRIC_TIMEOUT: Duration = Duration::from_secs(15);

/// 集合型搜索类型。
const COLLECTION_TYPES: [&str; 3] = ["album", "artist", "sheet"];

#[derive(Clone)]
pub struct MusicService {
    pool: SqlitePool,
    http: HttpClient,
    plugins: PluginRegistry,
    library: MusicLibrary,
    local: MusicLocalLibrary,
    cache: MusicCache,
}

impl MusicService {
    pub fn new(pool: SqlitePool, http: HttpClient, storage_dir: &str) -> Self {
        let plugins = PluginRegistry::new(pool.clone(), storage_dir, http.clone());
        let library = MusicLibrary::new(pool.clone());
        let local = MusicLocalLibrary::new(pool.clone());
        let cache = MusicCache::new(storage_dir, cache::DEFAULT_CACHE_LIMIT);
        Self {
            pool,
            http,
            plugins,
            library,
            local,
            cache,
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

    /// 按 URL 安装插件（支持 `.json` 订阅清单），返回逐条结果。
    pub async fn install_plugins_from_url(
        &self,
        url: &str,
    ) -> Result<Vec<PluginInstallResult>, AppError> {
        self.plugins.install_from_urls(url).await
    }

    pub async fn uninstall_all_plugins(&self) -> Result<u64, AppError> {
        self.plugins.uninstall_all().await
    }

    pub async fn reorder_plugins(&self, platforms: &[String]) -> Result<(), AppError> {
        self.plugins.reorder(platforms).await
    }

    pub async fn plugin_subscriptions(&self) -> Result<Vec<PluginSubscription>, AppError> {
        self.plugins.subscriptions().await
    }

    pub async fn add_plugin_subscription(
        &self,
        name: &str,
        url: &str,
    ) -> Result<PluginSubscription, AppError> {
        self.plugins.add_subscription(name, url).await
    }

    pub async fn remove_plugin_subscription(&self, id: i64) -> Result<bool, AppError> {
        self.plugins.remove_subscription(id).await
    }

    pub async fn update_plugin_subscription(
        &self,
        id: i64,
    ) -> Result<Vec<PluginInstallResult>, AppError> {
        self.plugins.update_subscription(id).await
    }

    pub async fn update_all_plugin_subscriptions(
        &self,
    ) -> Result<Vec<PluginInstallResult>, AppError> {
        self.plugins.update_all_subscriptions().await
    }

    pub async fn uninstall_plugin(&self, platform: &str) -> Result<bool, AppError> {
        self.plugins.uninstall(platform).await
    }

    pub async fn update_plugin(&self, platform: &str) -> Result<MusicPlugin, AppError> {
        self.plugins.update(platform).await
    }

    /// 返回 `{ updated, failures: [{ platform, error }] }`。
    pub async fn update_all_plugins(&self) -> Result<Value, AppError> {
        let (updated, failures) = self.plugins.update_all().await?;
        Ok(json!({
            "updated": updated,
            "failures": failures
                .into_iter()
                .map(|(platform, error)| json!({ "platform": platform, "error": error }))
                .collect::<Vec<_>>(),
        }))
    }

    pub async fn set_plugin_enabled(
        &self,
        platform: &str,
        enabled: bool,
    ) -> Result<bool, AppError> {
        self.plugins.set_enabled(platform, enabled).await
    }

    /// 插件声明的用户变量定义（登录 token 等）。
    pub async fn plugin_variable_definitions(
        &self,
        platform: &str,
    ) -> Result<Vec<PluginVariable>, AppError> {
        let plugin = self.require_plugin(platform).await?;
        self.plugins.variable_definitions(&plugin).await
    }

    pub async fn plugin_variable_values(
        &self,
        platform: &str,
    ) -> Result<HashMap<String, String>, AppError> {
        self.plugins.variable_values(platform).await
    }

    pub async fn set_plugin_variables(
        &self,
        platform: &str,
        values: &HashMap<String, String>,
    ) -> Result<(), AppError> {
        self.plugins.set_variables(platform, values).await
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

    /// 专辑 / 歌手 / 歌单搜索。插件协议里搜索是同一个方法, 只是 type 不同。
    pub async fn search_collections(
        &self,
        platform: &str,
        keyword: &str,
        page: u32,
        search_type: &str,
    ) -> Result<MusicCollectionPage, AppError> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(AppError::BadRequest("搜索关键词不能为空".to_string()));
        }
        if !COLLECTION_TYPES.contains(&search_type) {
            return Err(AppError::BadRequest(format!(
                "不支持的搜索类型: {search_type}"
            )));
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
        Ok(build_collection_page(
            search_type,
            &plugin.platform,
            &value,
        ))
    }

    /// 榜单列表：插件返回按组划分的榜单。
    pub async fn top_lists(&self, platform: &str) -> Result<Vec<MusicSheetGroup>, AppError> {
        let plugin = self.require_plugin(platform).await?;
        let value = self
            .plugins
            .call(&plugin, "getTopLists", json!([]), SEARCH_TIMEOUT)
            .await?;
        Ok(build_top_lists(&plugin.platform, &value))
    }

    /// 推荐歌单的 tag 列表；不支持该能力的插件会返回明确错误。
    pub async fn recommend_sheet_tags(
        &self,
        platform: &str,
    ) -> Result<MusicRecommendTags, AppError> {
        let plugin = self.require_plugin(platform).await?;
        let value = self
            .plugins
            .call(&plugin, "getRecommendSheetTags", json!([]), SEARCH_TIMEOUT)
            .await?;
        Ok(build_recommend_tags(&plugin.platform, &value))
    }

    /// 按 tag 分页取推荐歌单。
    pub async fn recommend_sheets(
        &self,
        platform: &str,
        tag: &MusicTag,
        page: u32,
    ) -> Result<MusicCollectionPage, AppError> {
        let plugin = self.require_plugin(platform).await?;
        let tag_value = serde_json::to_value(tag)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("无法序列化 tag: {e}")))?;
        let value = self
            .plugins
            .call(
                &plugin,
                "getRecommendSheetsByTag",
                json!([tag_value, page.max(1)]),
                SEARCH_TIMEOUT,
            )
            .await?;
        Ok(build_collection_page("sheet", &plugin.platform, &value))
    }

    /// 通过分享链接导入歌单，返回曲目列表（由前端决定存到哪个歌单）。
    pub async fn import_sheet(
        &self,
        platform: &str,
        url: &str,
    ) -> Result<Vec<MusicItem>, AppError> {
        let url = url.trim();
        if url.is_empty() {
            return Err(AppError::BadRequest("请填写分享链接".to_string()));
        }
        let plugin = self.require_plugin(platform).await?;
        let value = self
            .plugins
            .call(&plugin, "importMusicSheet", json!([url]), SEARCH_TIMEOUT)
            .await?;
        Ok(build_import_items(&plugin.platform, &value))
    }

    /// 通过分享链接导入单曲。
    pub async fn import_item(&self, platform: &str, url: &str) -> Result<MusicItem, AppError> {
        let url = url.trim();
        if url.is_empty() {
            return Err(AppError::BadRequest("请填写分享链接".to_string()));
        }
        let plugin = self.require_plugin(platform).await?;
        let value = self
            .plugins
            .call(&plugin, "importMusicItem", json!([url]), SEARCH_TIMEOUT)
            .await?;
        let data = value.get("data").unwrap_or(&value);
        normalize_item(&plugin.platform, data)
            .ok_or_else(|| AppError::BadRequest("链接没有解析出歌曲，可能链接无效".to_string()))
    }

    /// 集合详情：专辑 / 歌单 / 榜单 / 歌手作品。歌手用 `work_type` 区分单曲与专辑。
    pub async fn collection_detail(
        &self,
        platform: &str,
        kind: &str,
        item: &MusicCollectionItem,
        page: u32,
        work_type: &str,
    ) -> Result<MusicCollectionDetail, AppError> {
        let plugin = self.require_plugin(platform).await?;
        let item_value = serde_json::to_value(item).map_err(|e| {
            AppError::Internal(anyhow::anyhow!("无法序列化集合条目: {e}"))
        })?;
        let page = page.max(1);
        let (method, args, collection_key) = match kind {
            "album" => ("getAlbumInfo", json!([item_value, page]), "albumItem"),
            "sheet" => ("getMusicSheetInfo", json!([item_value, page]), "sheetItem"),
            "toplist" => ("getTopListDetail", json!([item_value, page]), "topListItem"),
            "artist" => (
                "getArtistWorks",
                json!([item_value, page, work_type]),
                "",
            ),
            other => {
                return Err(AppError::BadRequest(format!("不支持的详情类型: {other}")));
            }
        };
        let value = self
            .plugins
            .call(&plugin, method, args, SEARCH_TIMEOUT)
            .await?;
        Ok(build_collection_detail(
            &plugin.platform,
            kind,
            work_type,
            collection_key,
            &value,
        ))
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
    /// 命中本地缓存时不再走网络。
    pub async fn stream(
        &self,
        url: &str,
        headers: &serde_json::Map<String, Value>,
    ) -> Result<proxy::AudioStream, AppError> {
        if let Some((bytes, content_type)) = self.cache.lookup(url, headers) {
            return Ok(proxy::AudioStream {
                status: 200,
                content_type,
                bytes,
            });
        }
        let stream = proxy::fetch_audio(&self.http, url, headers).await?;
        self.cache
            .store(url, headers, &stream.bytes, stream.content_type.as_deref());
        Ok(stream)
    }

    /// 音乐缓存占用情况。
    pub fn cache_stats(&self) -> Value {
        let (bytes, files) = self.cache.stats();
        json!({
            "bytes": bytes,
            "files": files,
            "limitBytes": cache::DEFAULT_CACHE_LIMIT,
        })
    }

    /// 清空音乐缓存，返回释放的字节数。
    pub fn clear_cache(&self) -> Value {
        json!({ "freedBytes": self.cache.clear() })
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

/// 集合型搜索归一成结果页；兼容 `{ data: [...], isEnd }` 与直接数组。
fn build_collection_page(search_type: &str, platform: &str, value: &Value) -> MusicCollectionPage {
    let data = value.get("data").unwrap_or(value);
    let items = data
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|raw| normalize_collection(search_type, platform, raw))
                .collect()
        })
        .unwrap_or_default();
    let is_end = value.get("isEnd").and_then(Value::as_bool).unwrap_or(true);
    MusicCollectionPage {
        items,
        is_end,
        platform: platform.to_string(),
        search_type: search_type.to_string(),
    }
}

/// 榜单归一：`[{ title, data: [sheet] }]`。
fn build_top_lists(platform: &str, value: &Value) -> Vec<MusicSheetGroup> {
    let list = value
        .as_array()
        .or_else(|| value.get("data").and_then(Value::as_array));
    let Some(list) = list else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|raw| {
            let items: Vec<MusicCollectionItem> = raw
                .get("data")
                .or_else(|| raw.get("items"))
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| normalize_collection("sheet", platform, entry))
                        .collect()
                })
                .unwrap_or_default();
            if items.is_empty() {
                return None;
            }
            Some(MusicSheetGroup {
                title: text_field(raw, "title"),
                items,
            })
        })
        .collect()
}

/// 推荐歌单 tag 归一；tag 缺 id 时用标题兜底，保证能作为参数回传。
fn build_recommend_tags(platform: &str, value: &Value) -> MusicRecommendTags {
    let pinned = value
        .get("pinned")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|raw| normalize_collection("sheet", platform, raw))
                .collect()
        })
        .unwrap_or_default();
    let groups = value
        .get("data")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|raw| {
                    let tags: Vec<MusicTag> = raw
                        .get("data")
                        .or_else(|| raw.get("tags"))
                        .and_then(Value::as_array)
                        .map(|entries| {
                            entries
                                .iter()
                                .filter_map(|tag| {
                                    let mut title = text_field(tag, "title");
                                    if title.is_empty() {
                                        title = text_field(tag, "name");
                                    }
                                    if title.is_empty() {
                                        return None;
                                    }
                                    let mut id = text_field(tag, "id");
                                    if id.is_empty() {
                                        id = title.clone();
                                    }
                                    Some(MusicTag { id, title })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    if tags.is_empty() {
                        return None;
                    }
                    Some(MusicTagGroup {
                        title: text_field(raw, "title"),
                        tags,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    MusicRecommendTags { pinned, groups }
}

fn normalize_collection(
    search_type: &str,
    platform: &str,
    raw: &Value,
) -> Option<MusicCollectionItem> {
    if !raw.is_object() {
        return None;
    }
    let mut item: MusicCollectionItem = serde_json::from_value(raw.clone()).ok()?;
    if item.platform.is_empty() {
        item.platform = platform.to_string();
    }
    // 歌手条目用 name/avatar; 专辑、歌单用 title/artwork。
    if item.title.is_empty() {
        item.title = text_field(raw, "name");
    }
    if item.artwork.is_empty() {
        item.artwork = text_field(raw, "avatar");
    }
    if search_type == "artist" && item.description.is_empty() {
        item.description = text_field(raw, "description");
    }
    if item.id.is_empty() {
        return None;
    }
    Some(item)
}

fn text_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// 导入歌单的返回值：兼容直接数组与 `{ data: [...] }`。
fn build_import_items(platform: &str, value: &Value) -> Vec<MusicItem> {
    let data = value.get("data").unwrap_or(value);
    data.as_array()
        .map(|list| {
            list.iter()
                .filter_map(|raw| normalize_item(platform, raw))
                .collect()
        })
        .unwrap_or_default()
}

/// 集合详情归一：专辑/歌单取 musicList, 歌手作品取 data。
fn build_collection_detail(
    platform: &str,
    kind: &str,
    work_type: &str,
    collection_key: &str,
    value: &Value,
) -> MusicCollectionDetail {
    let is_end = value.get("isEnd").and_then(Value::as_bool).unwrap_or(true);
    let item = if collection_key.is_empty() {
        None
    } else {
        value
            .get(collection_key)
            .and_then(|raw| normalize_collection(kind, platform, raw))
    };
    let mut detail = MusicCollectionDetail {
        item,
        is_end,
        ..Default::default()
    };
    let list = value
        .get("musicList")
        .or_else(|| value.get("data"))
        .and_then(Value::as_array);
    if let Some(list) = list {
        if kind == "artist" && work_type == "album" {
            detail.collections = list
                .iter()
                .filter_map(|raw| normalize_collection("album", platform, raw))
                .collect();
        } else {
            detail.items = list
                .iter()
                .filter_map(|raw| normalize_item(platform, raw))
                .collect();
        }
    }
    detail
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

    #[test]
    fn normalizes_album_search_results() {
        let page = build_collection_page(
            "album",
            "demo",
            &json!({
                "isEnd": false,
                "data": [
                    { "id": "a1", "title": "专辑", "artist": "歌手", "worksNum": 12 },
                    { "title": "缺少 id" }
                ]
            }),
        );
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].platform, "demo");
        assert_eq!(page.items[0].works_num, Some(12.0));
        assert_eq!(page.search_type, "album");
        assert!(!page.is_end);
    }

    #[test]
    fn maps_artist_name_and_avatar() {
        let page = build_collection_page(
            "artist",
            "demo",
            &json!([{ "id": "s1", "name": "某歌手", "avatar": "https://a.png", "fans": 9 }]),
        );
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].title, "某歌手");
        assert_eq!(page.items[0].artwork, "https://a.png");
        assert_eq!(page.items[0].extra.get("fans").unwrap(), &json!(9));
        assert!(page.is_end);
    }

    #[test]
    fn parses_album_detail_with_music_list() {
        let detail = build_collection_detail(
            "demo",
            "album",
            "music",
            "albumItem",
            &json!({
                "isEnd": false,
                "albumItem": { "id": "a1", "title": "专辑", "artwork": "https://c.png" },
                "musicList": [
                    { "id": "m1", "title": "第一首" },
                    { "title": "缺少 id" }
                ]
            }),
        );
        assert_eq!(detail.item.as_ref().unwrap().title, "专辑");
        assert_eq!(detail.items.len(), 1);
        assert_eq!(detail.items[0].platform, "demo");
        assert!(!detail.is_end);
    }

    #[test]
    fn parses_artist_album_works_as_collections() {
        let detail = build_collection_detail(
            "demo",
            "artist",
            "album",
            "",
            &json!({ "data": [{ "id": "a1", "title": "专辑一" }] }),
        );
        assert!(detail.items.is_empty());
        assert_eq!(detail.collections.len(), 1);
        assert_eq!(detail.collections[0].title, "专辑一");
    }

    #[test]
    fn parses_artist_music_works_as_items() {
        let detail = build_collection_detail(
            "demo",
            "artist",
            "music",
            "",
            &json!({ "isEnd": true, "data": [{ "id": "m1", "title": "歌", "artist": "某人" }] }),
        );
        assert_eq!(detail.items.len(), 1);
        assert!(detail.collections.is_empty());
        assert!(detail.is_end);
    }

    #[test]
    fn parses_top_list_groups_and_drops_empty_groups() {
        let groups = build_top_lists(
            "demo",
            &json!([
                { "title": "热门", "data": [{ "id": "t1", "title": "飙升榜" }] },
                { "title": "空组", "data": [] }
            ]),
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "热门");
        assert_eq!(groups[0].items[0].title, "飙升榜");
        assert_eq!(groups[0].items[0].platform, "demo");
    }

    #[test]
    fn parses_recommend_tags_with_pinned_sheets() {
        let tags = build_recommend_tags(
            "demo",
            &json!({
                "pinned": [{ "id": "p1", "title": "每日推荐" }],
                "data": [
                    { "title": "风格", "data": [{ "id": "tag1", "title": "流行" }, { "title": "摇滚" }] }
                ]
            }),
        );
        assert_eq!(tags.pinned.len(), 1);
        assert_eq!(tags.groups.len(), 1);
        assert_eq!(tags.groups[0].tags.len(), 2);
        // 缺 id 的 tag 用标题兜底，保证还能作为参数回传。
        assert_eq!(tags.groups[0].tags[1].id, "摇滚");
    }

    #[test]
    fn parses_top_list_detail() {
        let detail = build_collection_detail(
            "demo",
            "toplist",
            "music",
            "topListItem",
            &json!({
                "topListItem": { "id": "t1", "title": "飙升榜" },
                "musicList": [{ "id": "m1", "title": "第一首" }]
            }),
        );
        assert_eq!(detail.item.as_ref().unwrap().title, "飙升榜");
        assert_eq!(detail.items.len(), 1);
    }

    #[test]
    fn parses_import_sheet_results() {
        let items = build_import_items(
            "demo",
            &json!([{ "id": "1", "title": "A" }, { "title": "无 id" }]),
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].platform, "demo");

        let wrapped = build_import_items("demo", &json!({ "data": [{ "id": "2", "title": "B" }] }));
        assert_eq!(wrapped.len(), 1);
        assert!(build_import_items("demo", &json!({})).is_empty());
    }
}
