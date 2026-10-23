//! 本地划线（highlight）命令。
//!
//! 存储与书签同一套 JSON 文档服务（`highlights.json`），这样备份/恢复、
//! 换机迁移与书签走同一条链路，不额外引入数据库迁移。

use crate::api::AppState;
use crate::error::error::{ApiResponse, AppError};
use crate::model::highlight::Highlight;
use serde_json::Value;

const STORE_FILE: &str = "highlights.json";

#[tauri::command]
pub async fn get_highlights(
    state: tauri::State<'_, AppState>,
) -> Result<ApiResponse<Value>, AppError> {
    let user_ns = "default";
    let list = state
        .json_document_service
        .read_list::<Highlight>(user_ns, STORE_FILE)
        .await?;
    Ok(ApiResponse::ok(
        serde_json::to_value(list).unwrap_or_default(),
    ))
}

#[tauri::command]
pub async fn save_highlight(
    state: tauri::State<'_, AppState>,
    req: Highlight,
) -> Result<ApiResponse<Value>, AppError> {
    let user_ns = "default";
    if req.book_url.trim().is_empty() || req.text.trim().is_empty() || req.block_index < 0 {
        return Err(AppError::BadRequest("划线数据无效".to_string()));
    }
    let mut list = state
        .json_document_service
        .read_list::<Highlight>(user_ns, STORE_FILE)
        .await?;
    let key = req.key();
    if let Some(position) = list.iter().position(|item| item.key() == key) {
        list[position] = req;
    } else {
        list.push(req);
    }
    state
        .json_document_service
        .write_list(user_ns, STORE_FILE, &list)
        .await?;
    Ok(ApiResponse::ok(Value::String(String::new())))
}

/// 批量 upsert：恢复备份时按 key 合并本地已有划线。
#[tauri::command]
pub async fn save_highlights(
    state: tauri::State<'_, AppState>,
    req: Vec<Highlight>,
) -> Result<ApiResponse<Value>, AppError> {
    let user_ns = "default";
    let mut list = state
        .json_document_service
        .read_list::<Highlight>(user_ns, STORE_FILE)
        .await?;
    for item in req {
        if item.book_url.trim().is_empty() || item.text.trim().is_empty() {
            continue;
        }
        let key = item.key();
        if let Some(position) = list.iter().position(|current| current.key() == key) {
            list[position] = item;
        } else {
            list.push(item);
        }
    }
    state
        .json_document_service
        .write_list(user_ns, STORE_FILE, &list)
        .await?;
    Ok(ApiResponse::ok(Value::String(String::new())))
}

/// 整表覆盖：覆盖恢复时先用备份内容替换，避免旧划线残留。
#[tauri::command]
pub async fn replace_highlights(
    state: tauri::State<'_, AppState>,
    req: Vec<Highlight>,
) -> Result<ApiResponse<Value>, AppError> {
    let user_ns = "default";
    let list: Vec<Highlight> = req
        .into_iter()
        .filter(|item| !item.book_url.trim().is_empty() && !item.text.trim().is_empty())
        .collect();
    state
        .json_document_service
        .write_list(user_ns, STORE_FILE, &list)
        .await?;
    Ok(ApiResponse::ok(Value::String(String::new())))
}

#[tauri::command]
pub async fn delete_highlight(
    state: tauri::State<'_, AppState>,
    req: Highlight,
) -> Result<ApiResponse<Value>, AppError> {
    let user_ns = "default";
    let mut list = state
        .json_document_service
        .read_list::<Highlight>(user_ns, STORE_FILE)
        .await?;
    let key = req.key();
    list.retain(|item| item.key() != key);
    state
        .json_document_service
        .write_list(user_ns, STORE_FILE, &list)
        .await?;
    Ok(ApiResponse::ok(Value::String(String::new())))
}

/// 删除一本书的全部划线（删除书籍时由前端显式调用）。
#[tauri::command]
pub async fn delete_book_highlights(
    state: tauri::State<'_, AppState>,
    book_url: String,
) -> Result<ApiResponse<Value>, AppError> {
    let user_ns = "default";
    let mut list = state
        .json_document_service
        .read_list::<Highlight>(user_ns, STORE_FILE)
        .await?;
    let before = list.len();
    list.retain(|item| item.book_url != book_url);
    let removed = before - list.len();
    state
        .json_document_service
        .write_list(user_ns, STORE_FILE, &list)
        .await?;
    Ok(ApiResponse::ok(serde_json::json!({ "removed": removed })))
}
