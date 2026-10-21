//! B 站弹幕: WebSocket + 自有二进制包头, body 为 JSON(protover 2 时 zlib 压缩)。
//!
//! 包结构(全部大端):
//! `[总长 u32][头长 u16=16][协议版本 u16][操作码 u32][序号 u32] + body`
//!
//! 操作码: 2 心跳 / 3 心跳回应(人气) / 5 通知 / 7 鉴权 / 8 鉴权回应。
//! 协议版本: 0 纯 JSON / 2 zlib 压缩 / 3 brotli。这里请求 protover=2, 只用 zlib,
//! 免去引入 brotli 依赖。

use futures::{SinkExt, StreamExt};
use md5::Digest;
use serde_json::Value;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use super::DanmakuMessage;

pub const PLATFORM_ID: &str = "bilibili";

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const HEARTBEAT_SECS: u64 = 30;
const FLUSH_MS: u64 = 300;
const MAX_BATCH: usize = 24;

struct DanmakuHost {
    token: String,
    host: String,
    buvid: String,
}

pub async fn run(
    http: &HttpClient,
    room_id: &str,
    channel: &tauri::ipc::Channel<Vec<DanmakuMessage>>,
    stop: Arc<AtomicBool>,
) -> Result<(), AppError> {
    let host = fetch_host(http, room_id).await?;
    let url = format!("wss://{}/sub", host.host);
    let (stream, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|err| AppError::BadRequest(format!("连接弹幕服务器失败: {err}")))?;
    let (mut sink, mut source) = stream.split();

    let auth = serde_json::json!({
        "uid": 0,
        "roomid": room_id.parse::<i64>().unwrap_or(0),
        "protover": 2,
        "platform": "web",
        "type": 2,
        "key": host.token,
        "buvid": host.buvid,
    });
    sink.send(Message::Binary(encode_packet(auth.to_string().as_bytes(), 7)))
        .await
        .map_err(|err| AppError::BadRequest(format!("弹幕鉴权失败: {err}")))?;

    let mut heartbeat = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
    let mut flush = tokio::time::interval(Duration::from_millis(FLUSH_MS));
    let mut batch: Vec<DanmakuMessage> = Vec::new();

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if sink.send(Message::Binary(encode_packet(b"", 2))).await.is_err() {
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

async fn fetch_host(http: &HttpClient, room_id: &str) -> Result<DanmakuHost, AppError> {
    // 该接口现在需要 WBI 签名(wts + w_rid), 裸请求会返回 -352 风控。
    let (img_key, sub_key) = wbi_keys(http).await?;
    let (buvid3, buvid4) = buvid(http).await?;
    let query = wbi_sign(
        &[("id", room_id.to_string()), ("type", "0".to_string())],
        &img_key,
        &sub_key,
    );
    let url = format!(
        "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo?{query}"
    );
    let value: Value = http
        .client()
        .get(&url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, "https://live.bilibili.com/")
        .header(
            reqwest::header::COOKIE,
            format!("buvid3={buvid3}; buvid4={buvid4}"),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if value.get("code").and_then(Value::as_i64).unwrap_or(0) != 0 {
        return Err(AppError::BadRequest(format!(
            "获取弹幕服务器失败: {}",
            value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("未知错误")
        )));
    }
    let token = value
        .pointer("/data/token")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let host = value
        .pointer("/data/host_list/0/host")
        .and_then(Value::as_str)
        .unwrap_or("broadcastlv.chat.bilibili.com")
        .to_string();
    let buvid = value
        .pointer("/data/host_list/0/buvid")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .unwrap_or(buvid3);
    if token.is_empty() {
        return Err(AppError::BadRequest("未获取到弹幕 token".to_string()));
    }
    Ok(DanmakuHost {
        token,
        host,
        buvid,
    })
}

/// WBI 混淆表(与服务端一致, 见 simple_live 的 `mixinKeyEncTab`)。
const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

async fn wbi_keys(http: &HttpClient) -> Result<(String, String), AppError> {
    let value: Value = http
        .client()
        .get("https://api.bilibili.com/x/web-interface/nav")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, "https://www.bilibili.com/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let img = value
        .pointer("/data/wbi_img/img_url")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let sub = value
        .pointer("/data/wbi_img/sub_url")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let key_of = |url: &str| -> String {
        url.rsplit('/')
            .next()
            .unwrap_or_default()
            .split('.')
            .next()
            .unwrap_or_default()
            .to_string()
    };
    let img_key = key_of(img);
    let sub_key = key_of(sub);
    if img_key.is_empty() || sub_key.is_empty() {
        return Err(AppError::BadRequest("未获取到 B站 WBI 密钥".to_string()));
    }
    Ok((img_key, sub_key))
}

/// 匿名设备指纹: 搜索/弹幕接口不带它会命中 -352。
async fn buvid(http: &HttpClient) -> Result<(String, String), AppError> {
    let value: Value = http
        .client()
        .get("https://api.bilibili.com/x/frontend/finger/spi")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, "https://www.bilibili.com/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let b3 = value
        .pointer("/data/b_3")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let b4 = value
        .pointer("/data/b_4")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok((b3, b4))
}

/// 生成 WBI 签名的查询串: 参数按 key 排序, 过滤 `!'()*`, 再拼 `wts` + `w_rid`。
fn wbi_sign(params: &[(&str, String)], img_key: &str, sub_key: &str) -> String {
    let origin = format!("{img_key}{sub_key}");
    let mixin: String = MIXIN_KEY_ENC_TAB
        .iter()
        .filter_map(|index| origin.chars().nth(*index))
        .take(32)
        .collect();

    let mut entries: Vec<(String, String)> = params
        .iter()
        .map(|(key, value)| {
            let cleaned: String = value
                .chars()
                .filter(|c| !"!'()*".contains(*c))
                .collect();
            ((*key).to_string(), cleaned)
        })
        .collect();
    entries.push(("wts".to_string(), chrono::Utc::now().timestamp().to_string()));
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let query = entries
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    let digest = md5::Md5::digest(format!("{query}{mixin}").as_bytes());
    format!("{query}&w_rid={}", hex::encode(digest))
}

pub fn encode_packet(body: &[u8], operation: u32) -> Vec<u8> {
    let total = (body.len() + 16) as u32;
    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(&total.to_be_bytes());
    out.extend_from_slice(&16u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&operation.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// 一个 WebSocket 帧里可能串联多个包; 压缩包的 body 解压后又是若干包。
pub fn decode_packets(data: &[u8], out: &mut Vec<DanmakuMessage>) {
    let mut offset = 0usize;
    while offset + 16 <= data.len() {
        let total = read_u32(data, offset) as usize;
        let header_len = read_u16(data, offset + 4) as usize;
        let version = read_u16(data, offset + 6);
        let operation = read_u32(data, offset + 8);
        if total < 16 || offset + total > data.len() {
            break;
        }
        let body = &data[offset + header_len.min(total)..offset + total];
        offset += total;

        if operation != 5 {
            continue;
        }
        match version {
            2 => {
                if let Some(plain) = inflate(body) {
                    decode_packets(&plain, out);
                }
            }
            3 => {
                // 服务端按 protover=2 回复; 万一回 3, 跳过而不是让整条连接断掉。
            }
            _ => parse_body(body, out),
        }
    }
}

fn inflate(body: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = flate2::read::ZlibDecoder::new(body);
    let mut plain = Vec::new();
    decoder.read_to_end(&mut plain).ok()?;
    Some(plain)
}

fn parse_body(body: &[u8], out: &mut Vec<DanmakuMessage>) {
    // 服务端可能给「多个 JSON 对象用 \x00 分隔」或「一个 JSON 数组」两种形态,
    // 先整体解析, 失败再按控制字符切分。
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        match value {
            Value::Array(items) => {
                for item in &items {
                    parse_command(item, out);
                }
                return;
            }
            Value::Object(_) => {
                parse_command(&value, out);
                return;
            }
            _ => {}
        }
    }

    let text = String::from_utf8_lossy(body);
    for chunk in text.split(['\x00', '\u{1}', '\u{2}', '\u{3}', '\u{4}', '\u{5}', '\u{6}', '\u{7}', '\u{8}', '\u{9}']) {
        let chunk = chunk.trim();
        if !chunk.starts_with('{') {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(chunk) {
            parse_command(&value, out);
        }
    }
}

fn parse_command(value: &Value, out: &mut Vec<DanmakuMessage>) {
    let cmd = value.get("cmd").and_then(Value::as_str).unwrap_or_default();
    if cmd.starts_with("DANMU_MSG") {
        let Some(info) = value.get("info").and_then(Value::as_array) else {
            return;
        };
        let content = info.get(1).and_then(Value::as_str).unwrap_or_default();
        let user = info
            .get(2)
            .and_then(Value::as_array)
            .and_then(|arr| arr.get(1))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let color = info
            .first()
            .and_then(|meta| meta.get(3))
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .map(|value| format!("#{:06x}", value & 0xffffff))
            .unwrap_or_default();
        if !content.is_empty() && !user.is_empty() {
            out.push(DanmakuMessage::chat(user.to_string(), content.to_string(), color));
        }
        return;
    }
    if cmd == "SEND_GIFT" {
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let user = data.get("uname").and_then(Value::as_str).unwrap_or_default();
        let gift = data.get("giftName").and_then(Value::as_str).unwrap_or_default();
        let num = data.get("num").and_then(Value::as_i64).unwrap_or(1);
        if !user.is_empty() && !gift.is_empty() {
            out.push(DanmakuMessage::system(
                "gift",
                user.to_string(),
                format!("送出 {gift} × {num}"),
            ));
        }
        return;
    }
    if cmd == "INTERACT_WORD" {
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let user = data.get("uname").and_then(Value::as_str).unwrap_or_default();
        if !user.is_empty() {
            out.push(DanmakuMessage::system("enter", user.to_string(), "进入直播间".to_string()));
        }
        return;
    }
    if cmd == "SUPER_CHAT_MESSAGE" {
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let user = data
            .pointer("/user_info/uname")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let message = data.get("message").and_then(Value::as_str).unwrap_or_default();
        let price = data.get("price").and_then(Value::as_i64).unwrap_or(0);
        // `time` 是 SC 的展示时长(秒), `start_time` 是生效时刻(unix 秒)。
        let duration = data.get("time").and_then(Value::as_i64).unwrap_or(0).max(0);
        let avatar = data
            .pointer("/user_info/face")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !message.is_empty() {
            out.push(DanmakuMessage::superchat(
                user.to_string(),
                message.to_string(),
                price,
                duration,
                avatar,
            ));
        }
    }
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([data[offset], data[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_heartbeat_with_fixed_16_byte_header() {
        let packet = encode_packet(b"", 2);
        assert_eq!(packet.len(), 16);
        assert_eq!(read_u32(&packet, 0), 16);
        assert_eq!(read_u16(&packet, 4), 16);
        assert_eq!(read_u32(&packet, 8), 2);
    }

    #[test]
    fn parses_chat_command() {
        let value = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [[0, 1, 25, 16777215], "你好", [1, "观众"]]
        });
        let mut out = Vec::new();
        parse_command(&value, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "chat");
        assert_eq!(out[0].user, "观众");
        assert_eq!(out[0].content, "你好");
        assert_eq!(out[0].color, "#ffffff");
    }

    #[test]
    fn parses_gift_and_enter() {
        let gift = serde_json::json!({
            "cmd": "SEND_GIFT",
            "data": { "uname": "土豪", "giftName": "辣条", "num": 10 }
        });
        let mut out = Vec::new();
        parse_command(&gift, &mut out);
        assert_eq!(out[0].kind, "gift");
        assert_eq!(out[0].content, "送出 辣条 × 10");

        let enter = serde_json::json!({
            "cmd": "INTERACT_WORD",
            "data": { "uname": "路人" }
        });
        parse_command(&enter, &mut out);
        assert_eq!(out[1].kind, "enter");
    }

    /// SC 带金额/时长/头像, 前端渲染独立的 SC 卡片横幅。
    #[test]
    fn parses_superchat_with_price_duration_and_avatar() {
        let value = serde_json::json!({
            "cmd": "SUPER_CHAT_MESSAGE",
            "data": {
                "user_info": { "uname": "老板", "face": "https://i0.hdslb.com/face.jpg" },
                "message": "唱一首歌", "price": 50, "time": 120, "start_time": 1760000000
            }
        });
        let mut out = Vec::new();
        parse_command(&value, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "superchat");
        assert_eq!(out[0].user, "老板");
        assert_eq!(out[0].content, "唱一首歌");
        assert_eq!(out[0].price, 50);
        assert_eq!(out[0].duration, 120);
        assert_eq!(out[0].avatar, "https://i0.hdslb.com/face.jpg");
    }

    /// 一个帧里串联两个 JSON 包时都要解出来(服务端会合并下发)。
    #[test]
    fn decodes_multiple_notification_packets() {
        let mut frame = Vec::new();
        let one = serde_json::json!({"cmd":"DANMU_MSG","info":[[0,1,25,0],"第一条",[1,"甲"]]});
        let two = serde_json::json!({"cmd":"DANMU_MSG","info":[[0,1,25,0],"第二条",[1,"乙"]]});
        frame.extend_from_slice(&encode_packet(one.to_string().as_bytes(), 5));
        frame.extend_from_slice(&encode_packet(two.to_string().as_bytes(), 5));
        let mut out = Vec::new();
        decode_packets(&frame, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].content, "第二条");
    }

    /// 压缩包: body 是 zlib, 解压后是若干完整包。
    #[test]
    fn decodes_zlib_wrapped_packets() {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write;

        let inner = serde_json::json!({"cmd":"DANMU_MSG","info":[[0,1,25,0],"压缩",[1,"丙"]]});
        let nested = encode_packet(inner.to_string().as_bytes(), 5);
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&nested).expect("compress");
        let compressed = encoder.finish().expect("finish");

        let mut packet = Vec::new();
        packet.extend_from_slice(&((compressed.len() + 16) as u32).to_be_bytes());
        packet.extend_from_slice(&16u16.to_be_bytes());
        packet.extend_from_slice(&2u16.to_be_bytes());
        packet.extend_from_slice(&5u32.to_be_bytes());
        packet.extend_from_slice(&1u32.to_be_bytes());
        packet.extend_from_slice(&compressed);

        let mut out = Vec::new();
        decode_packets(&packet, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, "压缩");
    }

    #[test]
    fn truncated_packets_do_not_panic() {
        let packet = encode_packet(b"{\"cmd\":\"X\"}", 5);
        let mut out = Vec::new();
        decode_packets(&packet[..10], &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn wbi_sign_is_stable_for_known_keys() {
        // 固定 key + 参数时, 除 wts 外的部分必须完全可复现。
        let query = wbi_sign(
            &[("id", "23335083".to_string()), ("type", "0".to_string())],
            "7cd084941338484aae1ad9425b84077c",
            "4932caff0ff746eab6f01bf08b70ac45",
        );
        assert!(query.starts_with("id=23335083&type=0&wts="), "query: {query}");
        assert!(query.contains("&w_rid="), "query: {query}");
        let w_rid = query.rsplit("w_rid=").next().unwrap_or_default();
        assert_eq!(w_rid.len(), 32, "md5 hex should be 32 chars");
    }

    /// 真实链路自检: 取 token → 连 WS → 鉴权 → 读若干帧。
    /// 需要外网, 默认忽略(`cargo test --lib -- --ignored danmaku`).
    #[tokio::test]
    #[ignore]
    async fn connects_and_reads_a_real_room() {
        let http = HttpClient::new(20, None).expect("http");
        // 用一个高人气在线房间, 保证 30 秒内一定有弹幕。
        let room_id = "1726390913";
        let host = fetch_host(&http, room_id).await.expect("danmu host");
        println!("host={} token_len={}", host.host, host.token.len());

        let url = format!("wss://{}/sub", host.host);
        let (mut stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("ws connect");
        let auth = serde_json::json!({
            "uid": 0, "roomid": room_id.parse::<i64>().unwrap_or(0), "protover": 2,
            "platform": "web", "type": 2, "key": host.token, "buvid": host.buvid,
        });
        stream
            .send(Message::Binary(encode_packet(auth.to_string().as_bytes(), 7)))
            .await
            .expect("send auth");

        let mut frames = 0;
        let mut messages = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        while frames < 12 && tokio::time::Instant::now() < deadline {
            let frame = tokio::time::timeout(Duration::from_secs(10), stream.next()).await;
            let Ok(Some(Ok(frame))) = frame else { break };
            frames += 1;
            match frame {
                Message::Binary(bytes) => {
                    let version = if bytes.len() >= 8 { read_u16(&bytes, 6) } else { 0 };
                    let operation = if bytes.len() >= 12 { read_u32(&bytes, 8) } else { 0 };
                    println!(
                        "frame {frames}: {} bytes ver={version} op={operation}",
                        bytes.len()
                    );
                    decode_packets(&bytes, &mut messages);
                }
                other => println!("frame {frames}: {other:?}"),
            }
        }
        println!("frames={frames} messages={}", messages.len());
        for message in messages.iter().take(3) {
            println!("  {}: {} {}", message.kind, message.user, message.content);
        }
        assert!(frames > 0, "should receive at least one frame");
    }
}
