//! 直播弹幕。
//!
//! 后端维持 WebSocket 长连接并解码, 通过 `tauri::ipc::Channel` 批量推给前端;
//! 前端只负责渲染。批量是为了避免每条弹幕一次 IPC(热门房间每秒几十条)。

pub mod bilibili;
pub mod douyin;
pub mod douyu;
pub mod huya;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

/// 一条弹幕/礼物/进场消息。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DanmakuMessage {
    /// `chat` | `gift` | `enter` | `superchat`
    pub kind: String,
    pub user: String,
    pub content: String,
    /// 前端直接当 CSS 颜色用的 `#rrggbb`; 空串表示用主题色。
    pub color: String,
    pub timestamp: i64,
}

impl DanmakuMessage {
    pub fn chat(user: String, content: String, color: String) -> Self {
        Self {
            kind: "chat".to_string(),
            user,
            content,
            color,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn system(kind: &str, user: String, content: String) -> Self {
        Self {
            kind: kind.to_string(),
            user,
            content,
            color: String::new(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// 当前正在收弹幕的房间, 用于「离开房间/切台」时停掉上一条连接。
#[derive(Clone, Default)]
pub struct DanmakuRegistry {
    sessions: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl DanmakuRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn register(&self, key: String) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        let mut sessions = self.sessions.lock().unwrap_or_else(|err| err.into_inner());
        // 同一房间重复开播时先把旧连接停掉, 否则会出现两条流。
        if let Some(previous) = sessions.insert(key, stop.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
        stop
    }

    pub fn stop(&self, key: &str) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(stop) = sessions.remove(key) {
            stop.store(true, Ordering::Relaxed);
        }
    }

    pub fn stop_all(&self) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|err| err.into_inner());
        for stop in sessions.values() {
            stop.store(true, Ordering::Relaxed);
        }
        sessions.clear();
    }
}

pub fn session_key(platform: &str, room_id: &str) -> String {
    format!("{platform}:{room_id}")
}

/// 启动一个房间的弹幕接收任务, 立即返回; 连接在后台维持。
#[allow(clippy::too_many_arguments)]
pub fn start(
    registry: &DanmakuRegistry,
    http: HttpClient,
    platform: String,
    room_id: String,
    channel: tauri::ipc::Channel<Vec<DanmakuMessage>>,
) -> Result<(), AppError> {
    if room_id.trim().is_empty() {
        return Err(AppError::BadRequest("缺少直播间号".to_string()));
    }
    if platform != bilibili::PLATFORM_ID
        && platform != huya::PLATFORM_ID
        && platform != douyu::PLATFORM_ID
        && platform != douyin::PLATFORM_ID
    {
        return Err(AppError::BadRequest(format!(
            "该平台暂不支持弹幕: {platform}"
        )));
    }
    let key = session_key(&platform, &room_id);
    let stop = registry.register(key.clone());
    let room = room_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = match platform.as_str() {
            huya::PLATFORM_ID => huya::run(&http, &room, &channel, stop.clone()).await,
            douyu::PLATFORM_ID => douyu::run(&http, &room, &channel, stop.clone()).await,
            douyin::PLATFORM_ID => douyin::run(&http, &room, &channel, stop.clone()).await,
            _ => bilibili::run(&http, &room, &channel, stop.clone()).await,
        };
        if let Err(err) = result {
            if !stop.load(Ordering::Relaxed) {
                let _ = channel.send(vec![DanmakuMessage::system(
                    "error",
                    String::new(),
                    format!("弹幕连接失败: {err}"),
                )]);
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registering_same_room_stops_the_previous_session() {
        let registry = DanmakuRegistry::new();
        let first = registry.register("bilibili:1".to_string());
        let second = registry.register("bilibili:1".to_string());
        assert!(first.load(Ordering::Relaxed));
        assert!(!second.load(Ordering::Relaxed));

        registry.stop("bilibili:1");
        assert!(second.load(Ordering::Relaxed));
    }

    #[test]
    fn rejects_unknown_platform() {
        // 只验证门禁分支, 不真的起连接。
        assert!(start(
            &DanmakuRegistry::new(),
            HttpClient::new(5, None).expect("http"),
            "unknown".to_string(),
            "1".to_string(),
            tauri::ipc::Channel::new(|_| Ok(())),
        )
        .is_err());
    }
}
