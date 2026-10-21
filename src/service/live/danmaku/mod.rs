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
use std::time::Duration;

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
    /// SC 金额(元); 非 SC 消息为 0。
    #[serde(default)]
    pub price: i64,
    /// SC 展示时长(秒); 非 SC 消息为 0。
    #[serde(default)]
    pub duration: i64,
    /// SC 发送者头像; 非 SC 消息为空串。
    #[serde(default)]
    pub avatar: String,
}

impl DanmakuMessage {
    pub fn chat(user: String, content: String, color: String) -> Self {
        Self {
            kind: "chat".to_string(),
            user,
            content,
            color,
            timestamp: chrono::Utc::now().timestamp_millis(),
            price: 0,
            duration: 0,
            avatar: String::new(),
        }
    }

    pub fn system(kind: &str, user: String, content: String) -> Self {
        Self {
            kind: kind.to_string(),
            user,
            content,
            color: String::new(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            price: 0,
            duration: 0,
            avatar: String::new(),
        }
    }

    /// SC 消息: 带金额/展示时长/头像, 前端渲染独立的 SC 卡片横幅。
    pub fn superchat(user: String, content: String, price: i64, duration: i64, avatar: String) -> Self {
        Self {
            kind: "superchat".to_string(),
            user,
            content,
            color: String::new(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            price,
            duration,
            avatar,
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
        // 直播平台的弹幕 WebSocket 会被服务端定期断开(也有网络抖动), 单次连接必然
        // 出现「弹幕突然不动了」。这里做常驻监督: 断开后按退避重连, 直到用户离开房间。
        let mut attempt: u32 = 0;
        loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let started = std::time::Instant::now();
            let result = match platform.as_str() {
                huya::PLATFORM_ID => huya::run(&http, &room, &channel, stop.clone()).await,
                douyu::PLATFORM_ID => douyu::run(&http, &room, &channel, stop.clone()).await,
                douyin::PLATFORM_ID => douyin::run(&http, &room, &channel, stop.clone()).await,
                _ => bilibili::run(&http, &room, &channel, stop.clone()).await,
            };
            if stop.load(Ordering::Relaxed) {
                return;
            }
            // 稳定跑过一段时间才断开 = 正常会话结束, 重置退避; 连上即断才累加退避。
            if started.elapsed() >= Duration::from_secs(30) {
                attempt = 0;
            }
            if let Err(err) = result {
                // 只在第一轮提示一次, 避免每次重连都往聊天区塞同一条错误。
                if attempt == 0 {
                    let _ = channel.send(vec![DanmakuMessage::system(
                        "error",
                        String::new(),
                        format!("弹幕连接中断，正在自动重连: {err}"),
                    )]);
                }
            }
            attempt = attempt.saturating_add(1);
            let delay = Duration::from_secs(std::cmp::min(1u64 << attempt.min(5), 30));
            let deadline = tokio::time::Instant::now() + delay;
            // 退避期间分段检查停止标志: 离开房间时不必等到退避结束。
            while tokio::time::Instant::now() < deadline {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
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
