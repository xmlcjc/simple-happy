use std::sync::atomic::{AtomicBool, Ordering};

use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 本轮关闭请求是否已被前端接管。
///
/// 外壳在拦截关闭请求后启动看门狗: 宽限期内前端若已 hide(关闭到托盘)或
/// destroy(退出), 就在这里置位, 看门狗据此退让; 否则替前端结束窗口, 避免
/// "窗口关不掉"。
///
/// 用显式回执而不是轮询窗口可见性: 隐藏到托盘后用户可能立刻从托盘重新打开
/// 窗口, 轮询会把那次重开误判成"前端没响应"并销毁窗口。
pub static CLOSE_REQUEST_HANDLED: AtomicBool = AtomicBool::new(false);

/// 前端回执: 这一轮关闭请求已经处理完(窗口已隐藏或已销毁)。
#[tauri::command]
pub fn acknowledge_close_request() {
    CLOSE_REQUEST_HANDLED.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn configure_boss_key(
    app: tauri::AppHandle,
    shortcut: Option<String>,
) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| error.to_string())?;
    let Some(shortcut) = shortcut
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if shortcut.split('+').any(|part| {
        matches!(
            part.trim().to_ascii_lowercase().as_str(),
            "super" | "meta" | "win"
        )
    }) {
        return Err("老板键不能使用系统键".to_string());
    }
    let parsed = shortcut
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|_| "快捷键格式无效".to_string())?;
    app.global_shortcut()
        .on_shortcut(parsed, |app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            if let Some(window) = app.get_webview_window("main") {
                if window.is_visible().unwrap_or(false) {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .map_err(|error| format!("快捷键注册失败：{error}"))
}
