//! 应用日志命令：读取 / 追加 / 清空。
//!
//! 日志文件由 `crate::util::log_file` 统一维护（`<storage_dir>/logs/app.log`，
//! 2 MB 轮换），Rust 端 tracing 与前端 `log_append` 写的是同一份文件，因此
//! 查看器里能看到一次失败前后的完整时间线。

use crate::api::AppState;
use crate::error::error::{ApiResponse, AppError};
use crate::util::log_file;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogAppendRequest {
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub tag: String,
    pub message: String,
}

/// 读取最近 `limit` 行（默认 1000，上限 5000）。
#[tauri::command]
pub async fn log_read(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
) -> Result<ApiResponse<Value>, AppError> {
    let (path, lines) = log_file::read_lines(&state.config.storage_dir, limit.unwrap_or(1000))
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;
    Ok(ApiResponse::ok(serde_json::json!({
        "path": path.to_string_lossy(),
        "lines": lines,
    })))
}

/// 前端写入一行日志（关键失败、播放/取流诊断采样）。
#[tauri::command]
pub async fn log_append(
    state: tauri::State<'_, AppState>,
    req: LogAppendRequest,
) -> Result<ApiResponse<Value>, AppError> {
    let line = log_file::format_line(&req.level, &req.tag, &req.message);
    log_file::append_line(&state.config.storage_dir, &line)
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;
    Ok(ApiResponse::ok(serde_json::json!({ "written": true })))
}

/// 清空当前日志与轮换文件。
#[tauri::command]
pub async fn log_clear(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Value>, AppError> {
    log_file::clear(&state.config.storage_dir)
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;
    Ok(ApiResponse::ok(serde_json::json!({ "cleared": true })))
}
