//! 斗鱼弹幕: WebSocket(8506) + STT 文本协议。
//!
//! 包结构(全部小端, 与 simple_live 的 `douyu_danmaku.dart` 一致):
//! `[长度 u32][长度 u32][类型 u16][加密 u8][保留 u8][STT 文本][\0]`
//! 长度 = 4 + 4 + 文本字节数 + 1(即不含自身 4 字节的整包长度)。
//! 类型: 客户端发 689, 服务端回 690。一个 WS 帧里可能串联多个包, 需循环拆解。
//!
//! 进房发 `loginreq` + `joingroup`(gid=-9999 是公共弹幕组), 45s 发 `mrkl` 心跳。
//! 弹幕是 `type@=chatmsg` 消息: nn=昵称 / txt=内容 / col=颜色(1-6);
//! `dms` 字段缺失的是非官方客户端弹幕, 按 simple_live 的做法直接过滤。

use futures::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use super::DanmakuMessage;

pub const PLATFORM_ID: &str = "douyu";

const SERVER_URL: &str = "wss://danmuproxy.douyu.com:8506";
const SERVER_HOST: &str = "danmuproxy.douyu.com";
const SERVER_PORT: u16 = 8506;
const HEARTBEAT_SECS: u64 = 45;
const FLUSH_MS: u64 = 300;
const MAX_BATCH: usize = 24;

pub async fn run(
    http: &HttpClient,
    room_id: &str,
    channel: &tauri::ipc::Channel<Vec<DanmakuMessage>>,
    stop: Arc<AtomicBool>,
) -> Result<(), AppError> {
    // 该服务器只支持旧 TLS 套件, rustls 握手必败; Windows 用 Schannel。
    #[cfg(target_os = "windows")]
    {
        let stream = connect_native_tls().await?;
        run_loop(stream, http, room_id, channel, stop).await
    }
    #[cfg(not(target_os = "windows"))]
    {
        let (stream, _) = tokio_tungstenite::connect_async(SERVER_URL)
            .await
            .map_err(|err| AppError::BadRequest(format!("连接斗鱼弹幕服务器失败: {err}")))?;
        run_loop(stream, http, room_id, channel, stop).await
    }
}

/// Windows: 手动 TCP + Schannel TLS, 再在 TLS 流上完成 WebSocket 握手。
#[cfg(target_os = "windows")]
async fn connect_native_tls() -> Result<
    WebSocketStream<tokio_native_tls::TlsStream<tokio::net::TcpStream>>,
    AppError,
> {
    let tcp = tokio::net::TcpStream::connect((SERVER_HOST, SERVER_PORT))
        .await
        .map_err(|err| AppError::BadRequest(format!("连接斗鱼弹幕服务器失败: {err}")))?;
    let connector = native_tls::TlsConnector::new()
        .map_err(|err| AppError::BadRequest(format!("初始化 TLS 失败: {err}")))?;
    let tls = tokio_native_tls::TlsConnector::from(connector)
        .connect(SERVER_HOST, tcp)
        .await
        .map_err(|err| AppError::BadRequest(format!("斗鱼弹幕 TLS 握手失败: {err}")))?;
    let (stream, _) = tokio_tungstenite::client_async(SERVER_URL, tls)
        .await
        .map_err(|err| AppError::BadRequest(format!("连接斗鱼弹幕服务器失败: {err}")))?;
    Ok(stream)
}

async fn run_loop<S>(
    stream: WebSocketStream<S>,
    _http: &HttpClient,
    room_id: &str,
    channel: &tauri::ipc::Channel<Vec<DanmakuMessage>>,
    stop: Arc<AtomicBool>,
) -> Result<(), AppError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut sink, mut source) = stream.split();

    let login = serialize(&format!("type@=loginreq/roomid@={room_id}/"));
    let join = serialize(&format!("type@=joingroup/rid@={room_id}/gid@=-9999/"));
    sink.send(Message::Binary(login))
        .await
        .map_err(|err| AppError::BadRequest(format!("斗鱼弹幕进房失败: {err}")))?;
    sink.send(Message::Binary(join))
        .await
        .map_err(|err| AppError::BadRequest(format!("斗鱼弹幕进组失败: {err}")))?;

    let mut heartbeat = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
    let mut flush = tokio::time::interval(Duration::from_millis(FLUSH_MS));
    let mut batch: Vec<DanmakuMessage> = Vec::new();

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if sink.send(Message::Binary(serialize("type@=mrkl/"))).await.is_err() {
                    break;
                }
            }
            _ = flush.tick() => {
                if !batch.is_empty() {
                    let payload = std::mem::take(&mut batch);
                    let _ = channel.send(payload);
                }
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
            }
            incoming = source.next() => {
                let Some(frame) = incoming else { break };
                let frame = frame.map_err(|err| AppError::BadRequest(format!("弹幕连接中断: {err}")))?;
                match frame {
                    Message::Binary(bytes) => decode_packets(&bytes, &mut batch),
                    Message::Text(text) => decode_packets(text.as_bytes(), &mut batch),
                    Message::Ping(payload) => {
                        let _ = sink.send(Message::Pong(payload)).await;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
                if batch.len() >= MAX_BATCH {
                    let payload = std::mem::take(&mut batch);
                    let _ = channel.send(payload);
                }
            }
        }
    }

    if !batch.is_empty() {
        let _ = channel.send(batch);
    }
    Ok(())
}

pub fn serialize(body: &str) -> Vec<u8> {
    let bytes = body.as_bytes();
    let len = (4 + 4 + bytes.len() + 1) as u32;
    let mut out = Vec::with_capacity(len as usize + 4);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&689u16.to_le_bytes());
    out.push(0);
    out.push(0);
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

/// 一个 WS 帧里可能串联多个斗鱼包, 循环拆解; 截断的尾巴直接丢弃。
pub fn decode_packets(data: &[u8], out: &mut Vec<DanmakuMessage>) {
    let mut offset = 0usize;
    while offset + 12 <= data.len() {
        let len = read_u32(data, offset) as usize;
        let total = len + 4;
        if len < 9 || offset + total > data.len() {
            break;
        }
        let body_end = offset + total - 1; // 末尾 \0 不算文本
        let body = String::from_utf8_lossy(&data[offset + 12..body_end]).into_owned();
        offset += total;
        parse_message(&body, out);
    }
}

/// STT 顶层字段: `key@=value/` 重复; 值里的 `/` 与 `@` 分别转义为 `@S`/`@A`。
fn parse_message(body: &str, out: &mut Vec<DanmakuMessage>) {
    let mut msg_type = "";
    let mut nick = "";
    let mut text = "";
    let mut color = "";
    let mut has_dms = false;
    for field in body.split('/') {
        let Some((key, value)) = field.split_once("@=") else {
            continue;
        };
        match key {
            "type" => msg_type = value,
            "nn" => nick = value,
            "txt" => text = value,
            "col" => color = value,
            "dms" => has_dms = true,
            _ => {}
        }
    }
    if msg_type != "chatmsg" || !has_dms || nick.is_empty() || text.is_empty() {
        return;
    }
    out.push(DanmakuMessage::chat(
        unescape(nick),
        unescape(text),
        color_hex(color).to_string(),
    ));
}

/// STT 反转义(顺序与 simple_live 相同: 先 `@S`→`/`, 再 `@A`→`@`)。
fn unescape(value: &str) -> String {
    value.replace("@S", "/").replace("@A", "@")
}

/// 斗鱼弹幕颜色编号, 与 simple_live 的 `getColor` 一致; 0/未知用主题色。
fn color_hex(col: &str) -> &'static str {
    match col {
        "1" => "#ff0000",
        "2" => "#1e87f0",
        "3" => "#7ac84b",
        "4" => "#ff7f00",
        "5" => "#9b39f4",
        "6" => "#ff69b4",
        _ => "",
    }
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_has_douyu_wire_layout() {
        let packet = serialize("type@=mrkl/");
        let len = read_u32(&packet, 0) as usize;
        assert_eq!(len, packet.len() - 4);
        assert_eq!(read_u32(&packet, 4) as usize, len);
        assert_eq!(u16::from_le_bytes([packet[8], packet[9]]), 689);
        assert_eq!(packet[10], 0);
        assert_eq!(packet[11], 0);
        assert_eq!(&packet[12..packet.len() - 1], b"type@=mrkl/");
        assert_eq!(packet[packet.len() - 1], 0);
    }

    #[test]
    fn decodes_chat_message_with_color() {
        let packet = serialize("type@=chatmsg/nn@=观众甲/txt@=主播好/col@=2/dms@=1/");
        let mut out = Vec::new();
        decode_packets(&packet, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "chat");
        assert_eq!(out[0].user, "观众甲");
        assert_eq!(out[0].content, "主播好");
        assert_eq!(out[0].color, "#1e87f0");
    }

    #[test]
    fn decodes_multiple_packets_in_one_frame() {
        let mut frame = serialize("type@=chatmsg/nn@=甲/txt@=一/col@=0/dms@=1/");
        frame.extend_from_slice(&serialize("type@=chatmsg/nn@=乙/txt@=二/col@=5/dms@=1/"));
        let mut out = Vec::new();
        decode_packets(&frame, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].content, "二");
        assert_eq!(out[1].color, "#9b39f4");
    }

    #[test]
    fn filters_messages_without_dms_and_other_types() {
        let mut out = Vec::new();
        // 无 dms 的「阴间弹幕」。
        decode_packets(&serialize("type@=chatmsg/nn@=甲/txt@=阴间/col@=0/"), &mut out);
        // 礼物/进房等其它类型暂不映射。
        decode_packets(&serialize("type@=dgb/nn@=甲/gf@=火箭/"), &mut out);
        decode_packets(&serialize("type@=uenter/nn@=甲/"), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn unescapes_stt_sequences() {
        let packet = serialize("type@=chatmsg/nn@=甲/txt@=a@Sb@Ac/col@=0/dms@=1/");
        let mut out = Vec::new();
        decode_packets(&packet, &mut out);
        assert_eq!(out[0].content, "a/b@c");
    }

    #[test]
    fn truncated_packets_do_not_panic() {
        let packet = serialize("type@=chatmsg/nn@=甲/txt@=好/col@=0/dms@=1/");
        let mut out = Vec::new();
        for cut in [1, 5, 11, packet.len() / 2, packet.len() - 1] {
            decode_packets(&packet[..cut], &mut out);
        }
        decode_packets(&[0xff; 3], &mut out);
    }

    /// 真实链路自检: 从斗鱼列表拿一个在播房间 → 进房 → 收弹幕。
    /// 需要外网, 默认忽略(`cargo test --lib -- --ignored danmaku`)。
    #[tokio::test]
    #[ignore]
    async fn connects_and_reads_a_real_room() {
        let http = HttpClient::new(20, None).expect("http");
        // 动态取一个当前在播的高人气房间(写死容易踩「主播未开播」)。
        let rooms = crate::service::live::douyu::Douyu::new(http.clone())
            .rooms("", "1", 1)
            .await
            .expect("room list");
        let room = rooms.first().expect("at least one live room");
        println!("room={} {}", room.room_id, room.title);

        // 端到端: 直接跑 run(), 用 Channel 闭包收集弹幕。
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink_messages = received.clone();
        let channel = tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(text) = body {
                if let Ok(batch) = serde_json::from_str::<Vec<DanmakuMessage>>(&text) {
                    sink_messages
                        .lock()
                        .unwrap_or_else(|err| err.into_inner())
                        .extend(batch);
                }
            }
            Ok(())
        });
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let http = http.clone();
            let stop = stop.clone();
            let room_id = room.room_id.clone();
            tokio::spawn(async move { run(&http, &room_id, &channel, stop).await })
        };

        // 收满 3 条或最多等 30 秒。
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        while tokio::time::Instant::now() < deadline {
            if received.lock().unwrap_or_else(|err| err.into_inner()).len() >= 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        stop.store(true, Ordering::Relaxed);
        let result = tokio::time::timeout(Duration::from_secs(5), worker).await;
        match &result {
            Ok(Ok(Ok(()))) => println!("run finished ok"),
            Ok(Ok(Err(err))) => println!("run error: {err}"),
            Ok(Err(err)) => println!("join error: {err}"),
            Err(_) => println!("worker did not stop in time"),
        }
        let messages = received.lock().unwrap_or_else(|err| err.into_inner());
        println!("messages={}", messages.len());
        for message in messages.iter().take(5) {
            println!("  {}: {} {}", message.kind, message.user, message.content);
        }
        assert!(!messages.is_empty(), "should decode at least one danmaku");
        drop(messages);
        assert!(
            result.expect("worker join").is_ok(),
            "run should not fail"
        );
    }
}
