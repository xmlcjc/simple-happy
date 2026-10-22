use crate::api::AppState;
use crate::error::error::{ApiResponse, AppError};
use crate::service::live::models::{
    LiveCategoryGroup, LivePlatform, LiveQrCode, LiveQrCodeStatus, LiveRoomDetail, LiveRoomSummary,
    LiveStreamInfo, LiveSuperChat,
};
use crate::service::live::danmaku::{self, DanmakuMessage};
use crate::service::live::follow::LiveFollow;
use crate::service::live::history::LiveHistoryEntry;
use crate::service::live::stats::{DailyLiveStats, LiveRoomStats, LiveStatsSummary};
use serde::Deserialize;

/// 当前支持的直播平台。前端据此渲染平台切换。
#[tauri::command]
pub async fn live_platforms(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Vec<LivePlatform>>, AppError> {
    Ok(ApiResponse::ok(state.live_service.platforms()))
}

#[tauri::command]
pub async fn live_categories(
    state: tauri::State<'_, AppState>,
    platform: String,
) -> Result<ApiResponse<Vec<LiveCategoryGroup>>, AppError> {
    Ok(ApiResponse::ok(
        state.live_service.categories(&platform).await?,
    ))
}

#[tauri::command]
pub async fn live_rooms(
    state: tauri::State<'_, AppState>,
    platform: String,
    parent_area_id: String,
    area_id: Option<String>,
    page: Option<u32>,
) -> Result<ApiResponse<Vec<LiveRoomSummary>>, AppError> {
    Ok(ApiResponse::ok(
        state
            .live_service
            .rooms(
                &platform,
                &parent_area_id,
                area_id.as_deref().unwrap_or("0"),
                page.unwrap_or(1),
            )
            .await?,
    ))
}

#[tauri::command]
pub async fn live_search(
    state: tauri::State<'_, AppState>,
    platform: String,
    keyword: String,
    page: Option<u32>,
) -> Result<ApiResponse<Vec<LiveRoomSummary>>, AppError> {
    Ok(ApiResponse::ok(
        state
            .live_service
            .search(&platform, &keyword, page.unwrap_or(1))
            .await?,
    ))
}

#[tauri::command]
pub async fn live_room_detail(
    state: tauri::State<'_, AppState>,
    platform: String,
    room_id: String,
) -> Result<ApiResponse<LiveRoomDetail>, AppError> {
    Ok(ApiResponse::ok(
        state.live_service.room_detail(&platform, &room_id).await?,
    ))
}

#[tauri::command]
pub async fn live_stream_urls(
    state: tauri::State<'_, AppState>,
    platform: String,
    room_id: String,
    cookie: Option<String>,
    qn: Option<i64>,
) -> Result<ApiResponse<LiveStreamInfo>, AppError> {
    Ok(ApiResponse::ok(
        state
            .live_service
            .stream_urls(&platform, &room_id, cookie.as_deref(), qn)
            .await?,
    ))
}

/// B 站扫码登录：生成二维码内容（前端渲染成图片）。
#[tauri::command]
pub async fn live_bilibili_qrcode(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<LiveQrCode>, AppError> {
    Ok(ApiResponse::ok(state.live_service.bilibili_qrcode().await?))
}

/// B 站扫码登录：轮询扫码状态，成功后返回可直接使用的登录 Cookie。
#[tauri::command]
pub async fn live_bilibili_qrcode_poll(
    state: tauri::State<'_, AppState>,
    qrcode_key: String,
) -> Result<ApiResponse<LiveQrCodeStatus>, AppError> {
    Ok(ApiResponse::ok(
        state.live_service.bilibili_qrcode_poll(&qrcode_key).await?,
    ))
}

/// 开始接收弹幕。后端维持长连接, 批量经 Channel 推送; 离开房间时调 stop。
#[tauri::command]
pub async fn live_danmaku_start(
    state: tauri::State<'_, AppState>,
    platform: String,
    room_id: String,
    on_event: tauri::ipc::Channel<Vec<DanmakuMessage>>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    danmaku::start(
        &state.danmaku_registry,
        state.live_service.http(),
        platform,
        room_id,
        on_event,
    )?;
    Ok(ApiResponse::ok(serde_json::json!({ "started": true })))
}

#[tauri::command]
pub async fn live_danmaku_stop(
    state: tauri::State<'_, AppState>,
    platform: String,
    room_id: String,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    state
        .danmaku_registry
        .stop(&danmaku::session_key(&platform, &room_id));
    Ok(ApiResponse::ok(serde_json::json!({ "stopped": true })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveFollowRequest {
    pub platform: String,
    pub room_id: String,
    pub room_name: Option<String>,
    pub anchor_name: Option<String>,
    pub avatar: Option<String>,
}

#[tauri::command]
pub async fn live_follow_room(
    state: tauri::State<'_, AppState>,
    req: LiveFollowRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let user_ns = "default";
    state
        .live_follow_service
        .follow(
            user_ns,
            &req.platform,
            &req.room_id,
            req.room_name.as_deref().unwrap_or_default(),
            req.anchor_name.as_deref().unwrap_or_default(),
            req.avatar.as_deref().unwrap_or_default(),
        )
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "saved": true })))
}

#[tauri::command]
pub async fn live_unfollow_room(
    state: tauri::State<'_, AppState>,
    platform: String,
    room_id: String,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let user_ns = "default";
    let removed = state
        .live_follow_service
        .unfollow(user_ns, &platform, &room_id)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "removed": removed })))
}

#[tauri::command]
pub async fn get_live_follows(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Vec<LiveFollow>>, AppError> {
    let user_ns = "default";
    Ok(ApiResponse::ok(
        state.live_follow_service.list(user_ns).await?,
    ))
}

/// 观看历史: 进房即记录(带封面/头像快照), 按最近访问倒序。
#[tauri::command]
pub async fn get_live_history(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<ApiResponse<Vec<LiveHistoryEntry>>, AppError> {
    let user_ns = "default";
    Ok(ApiResponse::ok(
        state
            .live_history_service
            .list(user_ns, limit.unwrap_or(50))
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLiveHistoryRequest {
    pub platform: String,
    pub room_id: String,
    pub room_name: Option<String>,
    pub anchor_name: Option<String>,
    pub cover: Option<String>,
    pub avatar: Option<String>,
}

/// 进房时上报一次观看历史(播放页拿到详情后调用)。
#[tauri::command]
pub async fn live_add_history(
    state: tauri::State<'_, AppState>,
    req: AddLiveHistoryRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let user_ns = "default";
    state
        .live_history_service
        .add(
            user_ns,
            &req.platform,
            &req.room_id,
            req.room_name.as_deref().unwrap_or_default(),
            req.anchor_name.as_deref().unwrap_or_default(),
            req.cover.as_deref().unwrap_or_default(),
            req.avatar.as_deref().unwrap_or_default(),
        )
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "saved": true })))
}

#[tauri::command]
pub async fn delete_live_history_room(
    state: tauri::State<'_, AppState>,
    platform: String,
    room_id: String,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let user_ns = "default";
    let deleted = state
        .live_history_service
        .remove(user_ns, &platform, &room_id)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "deleted": deleted })))
}

#[tauri::command]
pub async fn clear_live_history(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let user_ns = "default";
    let deleted = state.live_history_service.clear(user_ns).await?;
    Ok(ApiResponse::ok(serde_json::json!({ "deleted": deleted })))
}

/// 展示期内的 SC 列表(进房时读取, 与弹幕实时推送合并展示)。
#[tauri::command]
pub async fn get_live_superchat_history(
    state: tauri::State<'_, AppState>,
    platform: String,
    room_id: String,
) -> Result<ApiResponse<Vec<LiveSuperChat>>, AppError> {
    Ok(ApiResponse::ok(
        state.live_service.superchat_history(&platform, &room_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLiveStatsRequest {
    pub seconds: Option<i64>,
    pub sessions: Option<i64>,
    pub date: Option<String>,
    pub platform: String,
    pub room_id: String,
    pub room_name: Option<String>,
    pub anchor_name: Option<String>,
}

/// 累加一次直播观看时长（房间页定时上报）。
#[tauri::command]
pub async fn add_live_stats(
    state: tauri::State<'_, AppState>,
    req: AddLiveStatsRequest,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let user_ns = "default";
    state
        .live_stats_service
        .add_watch(
            user_ns,
            req.seconds.unwrap_or(0),
            req.sessions.unwrap_or(0),
            req.date.as_deref(),
            &req.platform,
            &req.room_id,
            req.room_name.as_deref().unwrap_or_default(),
            req.anchor_name.as_deref().unwrap_or_default(),
        )
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "saved": true })))
}

#[tauri::command]
pub async fn get_live_stats_daily(
    state: tauri::State<'_, AppState>,
    start: String,
    end: String,
) -> Result<ApiResponse<Vec<DailyLiveStats>>, AppError> {
    let user_ns = "default";
    Ok(ApiResponse::ok(
        state
            .live_stats_service
            .get_daily(user_ns, &start, &end)
            .await?,
    ))
}

#[tauri::command]
pub async fn get_live_stats_summary(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<LiveStatsSummary>, AppError> {
    let user_ns = "default";
    Ok(ApiResponse::ok(
        state.live_stats_service.get_summary(user_ns).await?,
    ))
}

#[tauri::command]
pub async fn get_live_stats_by_room(
    state: tauri::State<'_, AppState>,
    start: String,
    end: String,
) -> Result<ApiResponse<Vec<LiveRoomStats>>, AppError> {
    let user_ns = "default";
    Ok(ApiResponse::ok(
        state
            .live_stats_service
            .get_by_room(user_ns, &start, &end)
            .await?,
    ))
}

#[tauri::command]
pub async fn delete_live_stats_by_room(
    state: tauri::State<'_, AppState>,
    platform: String,
    room_id: String,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let user_ns = "default";
    let deleted = state
        .live_stats_service
        .delete_room(user_ns, &platform, &room_id)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "deleted": deleted })))
}
