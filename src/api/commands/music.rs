//! 音乐功能的 Tauri 命令层。
//!
//! 只做参数解包与 `MusicService` 转发, 不写业务逻辑。
//! 复合参数统一收进 `req`, 与 `commands::live` 的调用约定保持一致。

use std::collections::HashMap;

use serde::Deserialize;

use crate::api::AppState;
use crate::error::error::{ApiResponse, AppError};
use crate::service::music::library::{
    MusicHistoryEntry, MusicPlaylistSummary, MusicStatsOverview,
};
use crate::service::music::local::LocalTrack;
use crate::service::music::model::{
    MusicCollectionDetail, MusicCollectionItem, MusicCollectionPage, MusicItem, MusicLyric,
    MusicMediaSource, MusicPlugin, MusicRecommendTags, MusicSearchPage, MusicSheetGroup, MusicTag,
    PluginInstallResult, PluginSubscription, PluginVariable,
};

/// 当前为单用户桌面应用; 数据表仍保留 user_ns 维度, 便于日后多用户扩展。
const USER_NS: &str = "default";

#[tauri::command]
pub async fn music_plugin_list(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Vec<MusicPlugin>>, AppError> {
    Ok(ApiResponse::ok(state.music_service.list_plugins().await?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPluginInstallRequest {
    /// 插件脚本全文（前端通过文件选择框读取）。
    pub source: String,
    /// 原始文件名, 仅用于缺省展示名。
    pub file_name: Option<String>,
}

#[tauri::command]
pub async fn music_plugin_install(
    state: tauri::State<'_, AppState>,
    req: MusicPluginInstallRequest,
) -> Result<ApiResponse<MusicPlugin>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .install_plugin(&req.source, req.file_name.as_deref())
            .await?,
    ))
}

#[tauri::command]
pub async fn music_plugin_uninstall(
    state: tauri::State<'_, AppState>,
    platform: String,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let removed = state.music_service.uninstall_plugin(&platform).await?;
    Ok(ApiResponse::ok(serde_json::json!({ "removed": removed })))
}

/// 按 URL 安装：单个 `.js` 直链或 `.json` 订阅清单，逐条返回结果。
#[tauri::command]
pub async fn music_plugin_install_url(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<ApiResponse<Vec<PluginInstallResult>>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.install_plugins_from_url(&url).await?,
    ))
}

#[tauri::command]
pub async fn music_plugin_uninstall_all(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let removed = state.music_service.uninstall_all_plugins().await?;
    Ok(ApiResponse::ok(serde_json::json!({ "removed": removed })))
}

/// 按传入顺序重排插件。
#[tauri::command]
pub async fn music_plugin_reorder(
    state: tauri::State<'_, AppState>,
    platforms: Vec<String>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    state.music_service.reorder_plugins(&platforms).await?;
    Ok(ApiResponse::ok(serde_json::json!({ "reordered": platforms.len() })))
}

#[tauri::command]
pub async fn music_plugin_subscriptions(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Vec<PluginSubscription>>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.plugin_subscriptions().await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPluginSubscriptionRequest {
    #[serde(default)]
    pub name: String,
    pub url: String,
}

#[tauri::command]
pub async fn music_plugin_add_subscription(
    state: tauri::State<'_, AppState>,
    req: MusicPluginSubscriptionRequest,
) -> Result<ApiResponse<PluginSubscription>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .add_plugin_subscription(&req.name, &req.url)
            .await?,
    ))
}

#[tauri::command]
pub async fn music_plugin_remove_subscription(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let removed = state.music_service.remove_plugin_subscription(id).await?;
    Ok(ApiResponse::ok(serde_json::json!({ "removed": removed })))
}

#[tauri::command]
pub async fn music_plugin_update_subscription(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<ApiResponse<Vec<PluginInstallResult>>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.update_plugin_subscription(id).await?,
    ))
}

#[tauri::command]
pub async fn music_plugin_update_all_subscriptions(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Vec<PluginInstallResult>>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .update_all_plugin_subscriptions()
            .await?,
    ))
}

#[tauri::command]
pub async fn music_plugin_set_enabled(
    state: tauri::State<'_, AppState>,
    platform: String,
    enabled: bool,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let enabled = state
        .music_service
        .set_plugin_enabled(&platform, enabled)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "enabled": enabled })))
}

#[tauri::command]
pub async fn music_plugin_update(
    state: tauri::State<'_, AppState>,
    platform: String,
) -> Result<ApiResponse<MusicPlugin>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.update_plugin(&platform).await?,
    ))
}

#[tauri::command]
pub async fn music_plugin_update_all(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.update_all_plugins().await?,
    ))
}

#[tauri::command]
pub async fn music_plugin_user_variables(
    state: tauri::State<'_, AppState>,
    platform: String,
) -> Result<ApiResponse<Vec<PluginVariable>>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .plugin_variable_definitions(&platform)
            .await?,
    ))
}

#[tauri::command]
pub async fn music_plugin_variables(
    state: tauri::State<'_, AppState>,
    platform: String,
) -> Result<ApiResponse<HashMap<String, String>>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.plugin_variable_values(&platform).await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPluginVariablesRequest {
    pub platform: String,
    pub values: HashMap<String, String>,
}

#[tauri::command]
pub async fn music_plugin_set_variables(
    state: tauri::State<'_, AppState>,
    req: MusicPluginVariablesRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    state
        .music_service
        .set_plugin_variables(&req.platform, &req.values)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "saved": true })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicSearchRequest {
    pub platform: String,
    pub keyword: String,
    pub page: Option<u32>,
    /// 目前只支持 `music`; 预留 album/artist/sheet。
    pub search_type: Option<String>,
}

#[tauri::command]
pub async fn music_search(
    state: tauri::State<'_, AppState>,
    req: MusicSearchRequest,
) -> Result<ApiResponse<MusicSearchPage>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .search(
                &req.platform,
                &req.keyword,
                req.page.unwrap_or(1),
                req.search_type.as_deref().unwrap_or("music"),
            )
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicCollectionSearchRequest {
    pub platform: String,
    pub keyword: String,
    pub page: Option<u32>,
    /// album / artist / sheet。
    pub search_type: String,
}

#[tauri::command]
pub async fn music_search_collections(
    state: tauri::State<'_, AppState>,
    req: MusicCollectionSearchRequest,
) -> Result<ApiResponse<MusicCollectionPage>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .search_collections(
                &req.platform,
                &req.keyword,
                req.page.unwrap_or(1),
                &req.search_type,
            )
            .await?,
    ))
}

#[tauri::command]
pub async fn music_top_lists(
    state: tauri::State<'_, AppState>,
    platform: String,
) -> Result<ApiResponse<Vec<MusicSheetGroup>>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.top_lists(&platform).await?,
    ))
}

#[tauri::command]
pub async fn music_recommend_sheet_tags(
    state: tauri::State<'_, AppState>,
    platform: String,
) -> Result<ApiResponse<MusicRecommendTags>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.recommend_sheet_tags(&platform).await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicRecommendSheetsRequest {
    pub platform: String,
    pub tag: MusicTag,
    pub page: Option<u32>,
}

#[tauri::command]
pub async fn music_recommend_sheets(
    state: tauri::State<'_, AppState>,
    req: MusicRecommendSheetsRequest,
) -> Result<ApiResponse<MusicCollectionPage>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .recommend_sheets(&req.platform, &req.tag, req.page.unwrap_or(1))
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicImportRequest {
    pub platform: String,
    /// 分享链接（由插件解析）。
    pub url: String,
}

#[tauri::command]
pub async fn music_import_sheet(
    state: tauri::State<'_, AppState>,
    req: MusicImportRequest,
) -> Result<ApiResponse<Vec<MusicItem>>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .import_sheet(&req.platform, &req.url)
            .await?,
    ))
}

#[tauri::command]
pub async fn music_import_item(
    state: tauri::State<'_, AppState>,
    req: MusicImportRequest,
) -> Result<ApiResponse<MusicItem>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .import_item(&req.platform, &req.url)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicCollectionDetailRequest {
    pub platform: String,
    /// album / artist / sheet / toplist。
    pub kind: String,
    pub item: MusicCollectionItem,
    pub page: Option<u32>,
    /// 歌手详情专用: music / album。
    pub work_type: Option<String>,
}

#[tauri::command]
pub async fn music_collection_detail(
    state: tauri::State<'_, AppState>,
    req: MusicCollectionDetailRequest,
) -> Result<ApiResponse<MusicCollectionDetail>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .collection_detail(
                &req.platform,
                &req.kind,
                &req.item,
                req.page.unwrap_or(1),
                req.work_type.as_deref().unwrap_or("music"),
            )
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicMediaSourceRequest {
    pub platform: String,
    pub item: MusicItem,
    pub quality: Option<String>,
}

#[tauri::command]
pub async fn music_get_media_source(
    state: tauri::State<'_, AppState>,
    req: MusicMediaSourceRequest,
) -> Result<ApiResponse<MusicMediaSource>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .media_source(&req.platform, &req.item, req.quality.as_deref().unwrap_or("standard"))
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicLyricRequest {
    pub platform: String,
    pub item: MusicItem,
}

#[tauri::command]
pub async fn music_get_lyric(
    state: tauri::State<'_, AppState>,
    req: MusicLyricRequest,
) -> Result<ApiResponse<MusicLyric>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.lyric(&req.platform, &req.item).await?,
    ))
}

// ── 收藏 ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicFavoriteRequest {
    pub platform: String,
    pub item: MusicItem,
}

#[tauri::command]
pub async fn music_favorite_toggle(
    state: tauri::State<'_, AppState>,
    req: MusicFavoriteRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let favorited = state
        .music_service
        .library()
        .toggle_favorite(USER_NS, &req.item)
        .await?;
    Ok(ApiResponse::ok(
        serde_json::json!({ "favorited": favorited }),
    ))
}

#[tauri::command]
pub async fn music_favorite_list(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Vec<MusicItem>>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.library().favorites(USER_NS).await?,
    ))
}

#[tauri::command]
pub async fn music_favorite_check(
    state: tauri::State<'_, AppState>,
    platform: String,
    item_id: String,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let favorited = state
        .music_service
        .library()
        .is_favorite(USER_NS, &platform, &item_id)
        .await?;
    Ok(ApiResponse::ok(
        serde_json::json!({ "favorited": favorited }),
    ))
}

// ── 歌单 ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlaylistCreateRequest {
    pub name: String,
}

#[tauri::command]
pub async fn music_playlist_create(
    state: tauri::State<'_, AppState>,
    req: MusicPlaylistCreateRequest,
) -> Result<ApiResponse<MusicPlaylistSummary>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .library()
            .create_playlist(USER_NS, &req.name)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlaylistRenameRequest {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn music_playlist_rename(
    state: tauri::State<'_, AppState>,
    req: MusicPlaylistRenameRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    state
        .music_service
        .library()
        .rename_playlist(USER_NS, &req.id, &req.name)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "renamed": true })))
}

#[tauri::command]
pub async fn music_playlist_delete(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let deleted = state
        .music_service
        .library()
        .delete_playlist(USER_NS, &id)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "deleted": deleted })))
}

#[tauri::command]
pub async fn music_playlist_list(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Vec<MusicPlaylistSummary>>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.library().playlists(USER_NS).await?,
    ))
}

#[tauri::command]
pub async fn music_playlist_items(
    state: tauri::State<'_, AppState>,
    playlist_id: String,
) -> Result<ApiResponse<Vec<MusicItem>>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .library()
            .playlist_items(USER_NS, &playlist_id)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlaylistAddRequest {
    pub playlist_id: String,
    pub items: Vec<MusicItem>,
}

#[tauri::command]
pub async fn music_playlist_add(
    state: tauri::State<'_, AppState>,
    req: MusicPlaylistAddRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let added = state
        .music_service
        .library()
        .add_to_playlist(USER_NS, &req.playlist_id, &req.items)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "added": added })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlaylistRemoveRequest {
    pub playlist_id: String,
    pub platform: String,
    pub item_id: String,
}

#[tauri::command]
pub async fn music_playlist_remove(
    state: tauri::State<'_, AppState>,
    req: MusicPlaylistRemoveRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let removed = state
        .music_service
        .library()
        .remove_from_playlist(USER_NS, &req.playlist_id, &req.platform, &req.item_id)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "removed": removed })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlaylistKey {
    pub platform: String,
    pub item_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlaylistRemoveBatchRequest {
    pub playlist_id: String,
    pub keys: Vec<MusicPlaylistKey>,
}

#[tauri::command]
pub async fn music_playlist_remove_batch(
    state: tauri::State<'_, AppState>,
    req: MusicPlaylistRemoveBatchRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let keys: Vec<(String, String)> = req
        .keys
        .into_iter()
        .map(|key| (key.platform, key.item_id))
        .collect();
    let removed = state
        .music_service
        .library()
        .remove_many_from_playlist(USER_NS, &req.playlist_id, &keys)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "removed": removed })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlaylistReorderRequest {
    pub playlist_id: String,
    pub keys: Vec<MusicPlaylistKey>,
}

#[tauri::command]
pub async fn music_playlist_reorder(
    state: tauri::State<'_, AppState>,
    req: MusicPlaylistReorderRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let keys: Vec<(String, String)> = req
        .keys
        .into_iter()
        .map(|key| (key.platform, key.item_id))
        .collect();
    state
        .music_service
        .library()
        .reorder_playlist(USER_NS, &req.playlist_id, &keys)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "reordered": true })))
}

// ── 历史与统计 ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicHistoryRecordRequest {
    pub platform: String,
    pub item: MusicItem,
    pub seconds: Option<i64>,
    pub finished: Option<bool>,
}

#[tauri::command]
pub async fn music_history_record(
    state: tauri::State<'_, AppState>,
    req: MusicHistoryRecordRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    state
        .music_service
        .library()
        .record_history(
            USER_NS,
            &req.item,
            req.seconds.unwrap_or(0),
            req.finished.unwrap_or(false),
        )
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "saved": true })))
}

#[tauri::command]
pub async fn music_history_list(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<ApiResponse<Vec<MusicHistoryEntry>>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .library()
            .history(USER_NS, limit.unwrap_or(100))
            .await?,
    ))
}

#[tauri::command]
pub async fn music_history_clear(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let removed = state
        .music_service
        .library()
        .clear_history(USER_NS)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "removed": removed })))
}

#[tauri::command]
pub async fn music_stats_overview(
    state: tauri::State<'_, AppState>,
    days: Option<u32>,
) -> Result<ApiResponse<MusicStatsOverview>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .library()
            .stats_overview(USER_NS, days.unwrap_or(30))
            .await?,
    ))
}

// ── 本地音乐 ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicLocalImportRequest {
    pub paths: Vec<String>,
}

#[tauri::command]
pub async fn music_local_import(
    state: tauri::State<'_, AppState>,
    req: MusicLocalImportRequest,
) -> Result<ApiResponse<Vec<LocalTrack>>, AppError> {
    Ok(ApiResponse::ok(
        state
            .music_service
            .local()
            .import_paths(USER_NS, &req.paths)
            .await?,
    ))
}

/// 打开系统文件选择框并导入选中的音频文件。
#[tauri::command]
pub async fn music_local_pick(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Vec<LocalTrack>>, AppError> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .add_filter("音频文件", &["mp3", "flac", "m4a", "aac", "ogg", "wav", "opus"])
        .blocking_pick_files();
    let Some(files) = picked else {
        return Ok(ApiResponse::ok(Vec::new()));
    };
    let paths: Vec<String> = files
        .into_iter()
        .filter_map(|file| file.into_path().ok())
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    Ok(ApiResponse::ok(
        state
            .music_service
            .local()
            .import_paths(USER_NS, &paths)
            .await?,
    ))
}

#[tauri::command]
pub async fn music_local_list(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Vec<LocalTrack>>, AppError> {
    Ok(ApiResponse::ok(
        state.music_service.local().list(USER_NS).await?,
    ))
}

#[tauri::command]
pub async fn music_cache_stats(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    Ok(ApiResponse::ok(state.music_service.cache_stats()))
}

#[tauri::command]
pub async fn music_cache_clear(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    Ok(ApiResponse::ok(state.music_service.clear_cache()))
}

#[tauri::command]
pub async fn music_local_remove(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let removed = state
        .music_service
        .local()
        .remove(USER_NS, &path)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "removed": removed })))
}
