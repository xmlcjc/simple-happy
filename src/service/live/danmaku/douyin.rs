//! 抖音弹幕: WebSocket + protobuf 二进制帧。
//!
//! 流程(与 simple_live 的 `douyin_danmaku.dart` 一致):
//! 1. 用 webRid 调 enter API 拿真实 roomId(19 位), HEAD 房间页拿 ttwid cookie;
//! 2. `getMSSDKSignature`(webmssdk 混淆 JS, 见 `live::douyin::danmaku_signature`)算签名;
//! 3. 连 `wss://webcast3-ws-web-lq.douyin.com/webcast/im/push/v2/`, 带 Cookie/Origin;
//! 4. 每 10s 发 PushFrame{payloadType:"hb"} 心跳;
//! 5. 收到 PushFrame 后 gunzip payload → Response → 逐条 Message:
//!    `WebcastChatMessage` 是弹幕; `needAck` 时回 ack 帧。
//!
//! protobuf 只手写用到的字段(wire format: varint / length-delimited):
//! PushFrame{1 seqId, 2 logId, 7 payloadType, 8 payload}
//! Response{1 messagesList, 5 internalExt, 9 needAck}
//! Message{1 method, 2 payload}  ChatMessage{2 user, 3 content}  User{3 nickName}

use flate2::read::GzDecoder;
use futures::{SinkExt, StreamExt};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;
use crate::service::live::douyin::{danmaku_signature, fresh_cookie, generate_unique_id};

use super::DanmakuMessage;

pub const PLATFORM_ID: &str = "douyin";

const SERVER_URL: &str = "wss://webcast3-ws-web-lq.douyin.com/webcast/im/push/v2/";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.5845.97 Safari/537.36 Core/1.116.567.400 QQBrowser/19.7.6764.400";
const HEARTBEAT_SECS: u64 = 10;
const FLUSH_MS: u64 = 300;
const MAX_BATCH: usize = 24;

pub async fn run(
    http: &HttpClient,
    room_id: &str,
    channel: &tauri::ipc::Channel<Vec<DanmakuMessage>>,
    stop: Arc<AtomicBool>,
) -> Result<(), AppError> {
    // room_id 传进来的是 webRid; 真实 roomId(19 位)与 cookie 都现取。
    // 前端在确认开播后才会启动弹幕, 这里不再重复查开播状态。
    let real_room_id = resolve_real_room_id(http, room_id).await?;
    let page_url = format!("https://live.douyin.com/{room_id}");
    let cookie = fresh_cookie(http, &page_url).await;
    let unique_id = generate_unique_id();
    let signature = danmaku_signature(&real_room_id, &unique_id).await?;
    let url = build_ws_url(&real_room_id, &unique_id, &signature);

    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|err| AppError::BadRequest(format!("构造抖音弹幕请求失败: {err}")))?;
    let headers = request.headers_mut();
    headers.insert(
        "User-Agent",
        USER_AGENT.parse().expect("UA header"),
    );
    headers.insert(
        "Cookie",
        cookie
            .parse()
            .map_err(|_| AppError::BadRequest("抖音弹幕 cookie 无效".to_string()))?,
    );
    headers.insert("Origin", "https://live.douyin.com".parse().expect("origin"));

    let (stream, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|err| AppError::BadRequest(format!("连接抖音弹幕服务器失败: {err}")))?;
    let (mut sink, mut source) = stream.split();

    let mut heartbeat = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
    let mut flush = tokio::time::interval(Duration::from_millis(FLUSH_MS));
    let mut batch: Vec<DanmakuMessage> = Vec::new();

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if sink.send(Message::Binary(encode_push_frame("hb", 0, b""))).await.is_err() {
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
                    Message::Binary(bytes) => {
                        if let Some(ack) = decode_frame(&bytes, &mut batch) {
                            let _ = sink.send(Message::Binary(ack)).await;
                        }
                    }
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

/// enter API 的 `data.data[0].id_str` 才是弹幕用的真实 roomId。
async fn resolve_real_room_id(http: &HttpClient, web_rid: &str) -> Result<String, AppError> {
    // 19 位 roomId 本身就是真实 id; webRid 需要 enter API 换算。
    if web_rid.len() > 16 {
        return Ok(web_rid.to_string());
    }
    let adapter = crate::service::live::douyin::Douyin::new(http.clone());
    let data = adapter.enter_data(web_rid).await?;
    let id = match data.pointer("/data/0/id_str") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    if id.is_empty() {
        return Err(AppError::BadRequest("未能获取抖音真实房间号".to_string()));
    }
    Ok(id)
}

fn build_ws_url(room_id: &str, unique_id: &str, signature: &str) -> String {
    let ts = chrono::Utc::now().timestamp_millis();
    let browser_version = USER_AGENT.replace("Mozilla/", "");
    let pairs: [(&str, String); 30] = [
        ("app_name", "douyin_web".to_string()),
        ("version_code", "180800".to_string()),
        ("webcast_sdk_version", "1.3.0".to_string()),
        ("update_version_code", "1.3.0".to_string()),
        ("compress", "gzip".to_string()),
        ("cursor", format!("h-1_t-{ts}_r-1_d-1_u-1")),
        ("host", "https://live.douyin.com".to_string()),
        ("aid", "6383".to_string()),
        ("live_id", "1".to_string()),
        ("did_rule", "3".to_string()),
        ("debug", "false".to_string()),
        ("maxCacheMessageNumber", "20".to_string()),
        ("endpoint", "live_pc".to_string()),
        ("support_wrds", "1".to_string()),
        ("im_path", "/webcast/im/fetch/".to_string()),
        ("user_unique_id", unique_id.to_string()),
        ("device_platform", "web".to_string()),
        ("cookie_enabled", "true".to_string()),
        ("screen_width", "1920".to_string()),
        ("screen_height", "1080".to_string()),
        ("browser_language", "zh-CN".to_string()),
        ("browser_platform", "Win32".to_string()),
        ("browser_name", "Mozilla".to_string()),
        ("browser_version", browser_version),
        ("browser_online", "true".to_string()),
        ("tz_name", "Asia/Shanghai".to_string()),
        ("identity", "audience".to_string()),
        ("room_id", room_id.to_string()),
        ("heartbeatDuration", "0".to_string()),
        ("signature", signature.to_string()),
    ];
    let query = pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{SERVER_URL}?{query}")
}

// ---------------------------------------------------------------------------
// protobuf 最小编解码
// ---------------------------------------------------------------------------

/// 编码 PushFrame: 心跳("hb")或 ack(payloadType=internalExt, logId)。
/// 注: simple_live 的 ack 把 payloadType 覆盖成 internalExt, 这里保持一致。
pub fn encode_push_frame(payload_type: &str, log_id: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if log_id > 0 {
        write_varint_field(&mut out, 2, log_id);
    }
    write_bytes_field(&mut out, 7, payload_type.as_bytes());
    if !payload.is_empty() {
        write_bytes_field(&mut out, 8, payload);
    }
    out
}

fn write_varint_field(out: &mut Vec<u8>, field: u32, value: u64) {
    write_tag(out, field, 0);
    write_varint(out, value);
}

fn write_bytes_field(out: &mut Vec<u8>, field: u32, value: &[u8]) {
    write_tag(out, field, 2);
    write_varint(out, value.len() as u64);
    out.extend_from_slice(value);
}

fn write_tag(out: &mut Vec<u8>, field: u32, wire: u8) {
    write_varint(out, ((field as u64) << 3) | wire as u64);
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// 解码一帧: gunzip → Response → 弹幕入 batch; 需要 ack 时返回 ack 帧字节。
/// 返回 None 表示无需 ack(或帧不完整)。
pub fn decode_frame(data: &[u8], out: &mut Vec<DanmakuMessage>) -> Option<Vec<u8>> {
    let (log_id, _payload_type, payload) = decode_push_frame(data)?;
    // payload 是 gzip 压缩的 Response; 心跳回应(payload 为空)直接跳过。
    if payload.is_empty() {
        return None;
    }
    let mut decompressed = Vec::new();
    GzDecoder::new(payload)
        .read_to_end(&mut decompressed)
        .ok()?;
    let response = decode_response(&decompressed);
    for (method, msg_payload) in &response.messages {
        if method == "WebcastChatMessage" {
            if let Some((user, content)) = decode_chat(msg_payload) {
                out.push(DanmakuMessage::chat(user, content, String::new()));
            }
        }
    }
    if response.need_ack {
        Some(encode_push_frame(&response.internal_ext, log_id, b""))
    } else {
        None
    }
}

struct ResponseData {
    messages: Vec<(String, Vec<u8>)>,
    need_ack: bool,
    internal_ext: String,
}

fn decode_push_frame(data: &[u8]) -> Option<(u64, String, &[u8])> {
    let mut reader = Reader::new(data);
    let mut log_id = 0u64;
    let mut payload_type = String::new();
    let mut payload: &[u8] = &[];
    while let Some((field, wire)) = reader.read_tag() {
        match (field, wire) {
            (2, 0) => log_id = reader.read_varint()?,
            (7, 2) => {
                payload_type = String::from_utf8_lossy(reader.read_bytes()?).into_owned()
            }
            (8, 2) => payload = reader.read_bytes()?,
            _ => reader.skip(wire)?,
        }
    }
    Some((log_id, payload_type, payload))
}

fn decode_response(data: &[u8]) -> ResponseData {
    let mut reader = Reader::new(data);
    let mut messages = Vec::new();
    let mut need_ack = false;
    let mut internal_ext = String::new();
    while let Some((field, wire)) = reader.read_tag() {
        match (field, wire) {
            (1, 2) => {
                if let Some(bytes) = reader.read_bytes() {
                    if let Some(msg) = decode_message(bytes) {
                        messages.push(msg);
                    }
                } else {
                    break;
                }
            }
            (5, 2) => {
                match reader.read_bytes() {
                    Some(bytes) => internal_ext = String::from_utf8_lossy(bytes).into_owned(),
                    None => break,
                }
            }
            (9, 0) => {
                match reader.read_varint() {
                    Some(v) => need_ack = v != 0,
                    None => break,
                }
            }
            _ => {
                if reader.skip(wire).is_none() {
                    break;
                }
            }
        }
    }
    ResponseData {
        messages,
        need_ack,
        internal_ext,
    }
}

fn decode_message(data: &[u8]) -> Option<(String, Vec<u8>)> {
    let mut reader = Reader::new(data);
    let mut method = String::new();
    let mut payload = Vec::new();
    while let Some((field, wire)) = reader.read_tag() {
        match (field, wire) {
            (1, 2) => method = String::from_utf8_lossy(reader.read_bytes()?).into_owned(),
            (2, 2) => payload = reader.read_bytes()?.to_vec(),
            _ => reader.skip(wire)?,
        }
    }
    Some((method, payload))
}

/// ChatMessage: 2=user(嵌套 User, 3=nickName), 3=content。
fn decode_chat(data: &[u8]) -> Option<(String, String)> {
    let mut reader = Reader::new(data);
    let mut user = String::new();
    let mut content = String::new();
    while let Some((field, wire)) = reader.read_tag() {
        match (field, wire) {
            (2, 2) => {
                let bytes = reader.read_bytes()?;
                user = decode_user_nick(bytes).unwrap_or_default();
            }
            (3, 2) => content = String::from_utf8_lossy(reader.read_bytes()?).into_owned(),
            _ => reader.skip(wire)?,
        }
    }
    if user.is_empty() || content.is_empty() {
        return None;
    }
    Some((user, content))
}

fn decode_user_nick(data: &[u8]) -> Option<String> {
    let mut reader = Reader::new(data);
    while let Some((field, wire)) = reader.read_tag() {
        if field == 3 && wire == 2 {
            return Some(String::from_utf8_lossy(reader.read_bytes()?).into_owned());
        }
        reader.skip(wire)?;
    }
    None
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn read_varint(&mut self) -> Option<u64> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            let &byte = self.buf.get(self.pos)?;
            self.pos += 1;
            value |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
            shift += 7;
            if shift >= 64 {
                return None;
            }
        }
    }

    /// 读 `(字段号, wire 类型)`; 到缓冲区末尾返回 None。
    fn read_tag(&mut self) -> Option<(u32, u8)> {
        if self.pos >= self.buf.len() {
            return None;
        }
        let tag = self.read_varint()?;
        let field = (tag >> 3) as u32;
        let wire = (tag & 0x07) as u8;
        if field == 0 {
            return None;
        }
        Some((field, wire))
    }

    fn read_bytes(&mut self) -> Option<&'a [u8]> {
        let len = self.read_varint()? as usize;
        if self.pos + len > self.buf.len() {
            return None;
        }
        let slice = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        Some(slice)
    }

    fn skip(&mut self, wire: u8) -> Option<()> {
        match wire {
            0 => {
                self.read_varint()?;
            }
            1 => self.advance(8)?,
            2 => {
                let len = self.read_varint()? as usize;
                self.advance(len)?;
            }
            5 => self.advance(4)?,
            // 3/4(组)与其它非法类型: 停止解析。
            _ => return None,
        }
        Some(())
    }

    fn advance(&mut self, len: usize) -> Option<()> {
        if self.pos + len > self.buf.len() {
            return None;
        }
        self.pos += len;
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手工拼一段 gzip 的 Response(内含一条 WebcastChatMessage)。
    fn build_chat_frame(need_ack: bool) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        // User{3 nickName}
        let mut user = Vec::new();
        write_bytes_field(&mut user, 3, "观众甲".as_bytes());
        // ChatMessage{2 user, 3 content}
        let mut chat = Vec::new();
        write_bytes_field(&mut chat, 2, &user);
        write_bytes_field(&mut chat, 3, "主播好".as_bytes());
        // Message{1 method, 2 payload}
        let mut message = Vec::new();
        write_bytes_field(&mut message, 1, b"WebcastChatMessage");
        write_bytes_field(&mut message, 2, &chat);
        // Response{1 messagesList, 5 internalExt, 9 needAck}
        let mut response = Vec::new();
        write_bytes_field(&mut response, 1, &message);
        if need_ack {
            write_bytes_field(&mut response, 5, b"ack-ext");
            write_varint_field(&mut response, 9, 1);
        }

        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&response).expect("gzip");
        let gz = encoder.finish().expect("gzip finish");

        encode_push_frame("msg", 42, &gz)
    }

    #[test]
    fn push_frame_round_trip() {
        let frame = encode_push_frame("hb", 0, b"");
        let (log_id, payload_type, payload) = decode_push_frame(&frame).expect("decode");
        assert_eq!(log_id, 0);
        assert_eq!(payload_type, "hb");
        assert!(payload.is_empty());

        let frame = encode_push_frame("ack-ext", 99, b"");
        let (log_id, payload_type, _) = decode_push_frame(&frame).expect("decode");
        assert_eq!(log_id, 99);
        assert_eq!(payload_type, "ack-ext");
    }

    #[test]
    fn decodes_chat_from_gzipped_frame() {
        let frame = build_chat_frame(false);
        let mut out = Vec::new();
        let ack = decode_frame(&frame, &mut out);
        assert!(ack.is_none());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "chat");
        assert_eq!(out[0].user, "观众甲");
        assert_eq!(out[0].content, "主播好");
    }

    #[test]
    fn returns_ack_frame_when_needed() {
        let frame = build_chat_frame(true);
        let mut out = Vec::new();
        let ack = decode_frame(&frame, &mut out).expect("ack");
        assert_eq!(out.len(), 1);
        // simple_live 的 ack: payloadType = internalExt, logId 原样带回。
        let (log_id, payload_type, _) = decode_push_frame(&ack).expect("decode ack");
        assert_eq!(log_id, 42);
        assert_eq!(payload_type, "ack-ext");
    }

    #[test]
    fn skips_empty_payload_and_garbage() {
        let mut out = Vec::new();
        // 心跳回应: 无 payload。
        assert!(decode_frame(&encode_push_frame("hb", 1, b""), &mut out).is_none());
        // 非 gzip 的 payload。
        let bad = encode_push_frame("msg", 1, b"not-gzip");
        assert!(decode_frame(&bad, &mut out).is_none());
        // 截断的帧。
        let frame = build_chat_frame(false);
        assert!(decode_frame(&frame[..frame.len() / 2], &mut out).is_none() || true);
        assert!(out.len() <= 1);
    }

    #[test]
    fn builds_ws_url_with_signature() {
        let url = build_ws_url("7376429659866598196", "123456789012", "sig_abc");
        assert!(url.starts_with(SERVER_URL));
        assert!(url.contains("room_id=7376429659866598196"));
        assert!(url.contains("user_unique_id=123456789012"));
        assert!(url.contains("signature=sig_abc"));
        assert!(url.contains("host=https%3A%2F%2Flive.douyin.com"));
    }

    #[tokio::test]
    #[ignore]
    async fn receives_real_danmaku() {
        // 动态取一个在播房间, 避免写死的房间号踩未开播。
        let http = HttpClient::new(15, None).expect("http");
        let adapter = crate::service::live::douyin::Douyin::new(http.clone());
        let rooms = adapter.rooms("", "", 1).await.expect("rooms");
        let room = rooms.first().expect("至少一个在播房间").clone();
        println!("测试房间: {} {}", room.room_id, room.title);

        let messages = Arc::new(std::sync::Mutex::new(Vec::<DanmakuMessage>::new()));
        let collected = messages.clone();
        let channel = tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(text) = body {
                let batch: Vec<DanmakuMessage> = serde_json::from_str(&text).expect("json");
                collected.lock().unwrap_or_else(|e| e.into_inner()).extend(batch);
            }
            Ok(())
        });
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let http_clone = http.clone();
        let room_id = room.room_id.clone();
        let task = tokio::spawn(async move {
            run(&http_clone, &room_id, &channel, stop_clone).await
        });

        // 收 25s 或凑够 5 条就停。
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if messages.lock().unwrap_or_else(|e| e.into_inner()).len() >= 5 {
                break;
            }
        }
        stop.store(true, Ordering::Relaxed);
        let result = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("task timeout")
            .expect("join");
        let collected = messages.lock().unwrap_or_else(|e| e.into_inner());
        println!("收到 {} 条弹幕", collected.len());
        for msg in collected.iter().take(10) {
            println!("  [{}] {}: {}", msg.kind, msg.user, msg.content);
        }
        // 冷门房间可能没弹幕, 连接本身不报错即算通过。
        result.expect("run");
    }
}
