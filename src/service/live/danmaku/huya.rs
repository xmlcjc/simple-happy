//! 虎牙弹幕: WebSocket + TARS(JCE) 二进制编解码。
//!
//! 流程(与 simple_live 的 `huya_danmaku.dart` 一致):
//! 1. 手机页 `m.huya.com/{roomId}` 的 HTML 里正则取 `lYyid`/`lChannelId`/`lSubChannelId`;
//! 2. 连 `wss://cdnws.api.huya.com`, 发 TARS 编码的 WSRegisterReq 进房;
//! 3. 每 60s 发 9 字节固定心跳;
//! 4. 收到 cmd=7 的推送后解 HYPushMessage, uri=1400 是弹幕(HYMessage)。
//!
//! TARS 是腾讯 JCE 二进制协议: 每个字段一个头部字节 `(tag << 4) | type`,
//! tag >= 15 时头部两字节(首字节 `0xF0 | type`, 次字节为 tag)。整数按范围选
//! byte/short/int/long 四种类型, 解码端按类型字节还原, 因此编码边界不影响兼容。

use futures::{SinkExt, StreamExt};
use regex::Regex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

use super::DanmakuMessage;

pub const PLATFORM_ID: &str = "huya";

/// 抓手机页需要移动端 UA(与 simple_live 相同)。
const MOBILE_UA: &str =
    "Mozilla/5.0 (Linux; Android 11; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.91 Mobile Safari/537.36 Edg/117.0.0.0";
const SERVER_URL: &str = "wss://cdnws.api.huya.com";
const HEARTBEAT_SECS: u64 = 60;
const FLUSH_MS: u64 = 300;
const MAX_BATCH: usize = 24;

/// 固定心跳包(simple_live 里的 `base64.decode("ABQdAAwsNgBM")`)。
const HEARTBEAT: [u8; 9] = [0x00, 0x14, 0x1d, 0x00, 0x0c, 0x2c, 0x36, 0x00, 0x4c];

struct HuyaArgs {
    yyid: i64,
    top_sid: i64,
    sub_sid: i64,
}

pub async fn run(
    http: &HttpClient,
    room_id: &str,
    channel: &tauri::ipc::Channel<Vec<DanmakuMessage>>,
    stop: Arc<AtomicBool>,
) -> Result<(), AppError> {
    let args = fetch_args(http, room_id).await?;
    let (stream, _) = tokio_tungstenite::connect_async(SERVER_URL)
        .await
        .map_err(|err| AppError::BadRequest(format!("连接虎牙弹幕服务器失败: {err}")))?;
    let (mut sink, mut source) = stream.split();

    sink.send(Message::Binary(encode_register(args.yyid, args.top_sid, args.sub_sid)))
        .await
        .map_err(|err| AppError::BadRequest(format!("虎牙弹幕进房失败: {err}")))?;

    let mut heartbeat = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
    let mut flush = tokio::time::interval(Duration::from_millis(FLUSH_MS));
    let mut batch: Vec<DanmakuMessage> = Vec::new();

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if sink.send(Message::Binary(HEARTBEAT.to_vec())).await.is_err() {
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
                    Message::Binary(bytes) => decode_frame(&bytes, &mut batch),
                    Message::Text(text) => decode_frame(text.as_bytes(), &mut batch),
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

/// 进房所需的三个 ID 不在任何公开 JSON 接口里, 只能从房间手机页 HTML 正则。
async fn fetch_args(http: &HttpClient, room_id: &str) -> Result<HuyaArgs, AppError> {
    let url = format!("https://m.huya.com/{room_id}");
    let html = http
        .client()
        .get(&url)
        .header(reqwest::header::USER_AGENT, MOBILE_UA)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let yyid = find_id(&html, r#""lYyid"\s*:\s*(\d+)"#).unwrap_or(0);
    let top_sid = find_id(&html, r#""lChannelId"\s*:\s*(\d+)"#).unwrap_or(0);
    let sub_sid = find_id(&html, r#""lSubChannelId"\s*:\s*(\d+)"#).unwrap_or(0);
    if yyid == 0 || top_sid == 0 {
        return Err(AppError::BadRequest(format!(
            "未从房间页取到弹幕参数(直播间 {room_id} 可能未开播)"
        )));
    }
    // 个别房间页缺 lSubChannelId 时, 虎牙服务端接受与频道号相同的 sid
    // (simple_live 干脆两个字段都传 topSid)。
    let sub_sid = if sub_sid > 0 { sub_sid } else { top_sid };
    Ok(HuyaArgs {
        yyid,
        top_sid,
        sub_sid,
    })
}

fn find_id(html: &str, pattern: &str) -> Option<i64> {
    let regex = Regex::new(pattern).ok()?;
    regex
        .captures(html)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

// ---------------------------------------------------------------------------
// TARS 最小编码器
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Self::default()
    }

    fn head(&mut self, ty: u8, tag: u8) {
        if tag < 15 {
            self.buf.push((tag << 4) | ty);
        } else {
            self.buf.push(0xf0 | ty);
            self.buf.push(tag);
        }
    }

    fn write_int(&mut self, value: i64, tag: u8) {
        if let Ok(v) = i8::try_from(value) {
            self.head(0, tag);
            self.buf.push(v as u8);
        } else if let Ok(v) = i16::try_from(value) {
            self.head(1, tag);
            self.buf.extend_from_slice(&v.to_be_bytes());
        } else if let Ok(v) = i32::try_from(value) {
            self.head(2, tag);
            self.buf.extend_from_slice(&v.to_be_bytes());
        } else {
            self.head(3, tag);
            self.buf.extend_from_slice(&value.to_be_bytes());
        }
    }

    fn write_bool(&mut self, value: bool, tag: u8) {
        self.head(0, tag);
        self.buf.push(u8::from(value));
    }

    fn write_string(&mut self, value: &str, tag: u8) {
        let bytes = value.as_bytes();
        if bytes.len() <= 255 {
            self.head(6, tag);
            self.buf.push(bytes.len() as u8);
        } else {
            self.head(7, tag);
            self.buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        }
        self.buf.extend_from_slice(bytes);
    }

    /// 字节数组(simple list): 头部后跟一个类型 0 的子头部 + 长度(int) + 原始字节。
    fn write_bytes(&mut self, value: &[u8], tag: u8) {
        self.head(13, tag);
        self.head(0, 0);
        self.write_int(value.len() as i64, 0);
        self.buf.extend_from_slice(value);
    }

    fn struct_begin(&mut self, tag: u8) {
        self.head(10, tag);
    }

    fn struct_end(&mut self) {
        self.head(11, 0);
    }
}

/// WSRegisterReq: simple_live 里 tid/sid 都传 topSid, 这里用
/// (topSid, subSid) 并在 subSid 缺失时回退 topSid(见 `fetch_args`)。
pub fn encode_register(uid: i64, tid: i64, sid: i64) -> Vec<u8> {
    let mut inner = Writer::new();
    inner.write_int(uid, 0);
    inner.write_bool(true, 1);
    inner.write_string("", 2);
    inner.write_string("", 3);
    inner.write_int(tid, 4);
    inner.write_int(sid, 5);
    inner.write_int(0, 6);
    inner.write_int(0, 7);

    let mut outer = Writer::new();
    outer.write_int(1, 0); // iCmdType = 1: 注册
    outer.write_bytes(&inner.buf, 1);
    outer.buf
}

// ---------------------------------------------------------------------------
// TARS 最小解码器
// ---------------------------------------------------------------------------

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// 读 `(类型, tag, 头部长度)`, 不移动游标。
    fn peek_head(&self) -> Option<(u8, u8, usize)> {
        let &first = self.buf.get(self.pos)?;
        let ty = first & 0x0f;
        let tag = first >> 4;
        if tag == 15 {
            let &second = self.buf.get(self.pos + 1)?;
            Some((ty, second, 2))
        } else {
            Some((ty, tag, 1))
        }
    }

    /// 把游标推进到目标 tag 的头部位置, 返回 `(类型, 头部长度)`。
    /// 找不到(遇到更大 tag / 结构体结束 / 越界)返回 None。
    fn skip_to_tag(&mut self, target: u8) -> Option<(u8, usize)> {
        loop {
            let (ty, tag, head_len) = self.peek_head()?;
            if ty == 11 || tag > target {
                return None;
            }
            if tag == target {
                return Some((ty, head_len));
            }
            self.pos += head_len;
            self.skip_field(ty)?;
        }
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn read_int(&mut self, tag: u8) -> i64 {
        let Some((ty, head_len)) = self.skip_to_tag(tag) else {
            return 0;
        };
        self.pos += head_len;
        match ty {
            0 => self.take(1).map(|b| b[0] as i8 as i64).unwrap_or(0),
            1 => self
                .take(2)
                .map(|b| i16::from_be_bytes([b[0], b[1]]) as i64)
                .unwrap_or(0),
            2 => self
                .take(4)
                .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64)
                .unwrap_or(0),
            3 => self
                .take(8)
                .map(|b| i64::from_be_bytes(b.try_into().unwrap_or_default()))
                .unwrap_or(0),
            // 12 = zero, 其它类型按 0 处理(不崩溃, 容错优先)。
            _ => 0,
        }
    }

    fn read_string(&mut self, tag: u8) -> String {
        let Some((ty, head_len)) = self.skip_to_tag(tag) else {
            return String::new();
        };
        self.pos += head_len;
        let bytes = match ty {
            6 => {
                let Some(&[len]) = self.take(1) else { return String::new() };
                self.take(len as usize)
            }
            7 => {
                let Some(len) = self.take(4).map(|b| {
                    u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize
                }) else {
                    return String::new();
                };
                self.take(len)
            }
            _ => None,
        };
        bytes
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default()
    }

    fn read_bytes(&mut self, tag: u8) -> Option<Vec<u8>> {
        let (ty, head_len) = self.skip_to_tag(tag)?;
        self.pos += head_len;
        if ty != 13 {
            return None;
        }
        // 子头部(类型 0) + 长度(int)。
        let (_, _, sub_head_len) = self.peek_head()?;
        self.pos += sub_head_len;
        let size = self.read_int(0);
        let size = usize::try_from(size).ok()?;
        self.take(size).map(|b| b.to_vec())
    }

    /// 读嵌套结构体为子切片(不含结束标记), 游标移到结构体结束之后。
    fn read_struct(&mut self, tag: u8) -> Option<Reader<'a>> {
        let (ty, head_len) = self.skip_to_tag(tag)?;
        self.pos += head_len;
        if ty != 10 {
            return None;
        }
        let start = self.pos;
        loop {
            let (inner_ty, _, inner_head_len) = self.peek_head()?;
            if inner_ty == 11 {
                let end = self.pos;
                self.pos += inner_head_len;
                return Some(Reader::new(&self.buf[start..end]));
            }
            self.pos += inner_head_len;
            self.skip_field(inner_ty)?;
        }
    }

    /// 按类型跳过一个完整字段值(此时头部已被消费)。
    fn skip_field(&mut self, ty: u8) -> Option<()> {
        match ty {
            0 | 12 => {
                if ty == 0 {
                    self.take(1)?;
                }
            }
            1 => {
                self.take(2)?;
            }
            2 | 4 => {
                self.take(4)?;
            }
            3 | 5 => {
                self.take(8)?;
            }
            6 => {
                let &[len] = self.take(1)? else { return None };
                self.take(len as usize)?;
            }
            7 => {
                let len = u32::from_be_bytes(self.take(4)?.try_into().ok()?) as usize;
                self.take(len)?;
            }
            8 => {
                // map: 长度后有 2*size 个「头+值」。
                let size = self.read_int(0);
                for _ in 0..size.max(0) * 2 {
                    let (elem_ty, _, head_len) = self.peek_head()?;
                    self.pos += head_len;
                    self.skip_field(elem_ty)?;
                }
            }
            9 => {
                let size = self.read_int(0);
                for _ in 0..size.max(0) {
                    let (elem_ty, _, head_len) = self.peek_head()?;
                    self.pos += head_len;
                    self.skip_field(elem_ty)?;
                }
            }
            10 => {
                loop {
                    let (inner_ty, _, head_len) = self.peek_head()?;
                    self.pos += head_len;
                    if inner_ty == 11 {
                        break;
                    }
                    self.skip_field(inner_ty)?;
                }
            }
            11 => {}
            13 => {
                let (_, _, sub_head_len) = self.peek_head()?;
                self.pos += sub_head_len;
                let size = self.read_int(0);
                self.take(usize::try_from(size).ok()?)?;
            }
            _ => return None,
        }
        Some(())
    }
}

/// 外层帧: tag 0 是 cmd, 7 表示推送; tag 1 是 HYPushMessage 字节。
pub fn decode_frame(data: &[u8], out: &mut Vec<DanmakuMessage>) {
    let mut reader = Reader::new(data);
    if reader.read_int(0) != 7 {
        return;
    }
    let Some(push) = reader.read_bytes(1) else {
        return;
    };
    let mut push_reader = Reader::new(&push);
    let _push_type = push_reader.read_int(0);
    let uri = push_reader.read_int(1);
    let msg = push_reader.read_bytes(2).unwrap_or_default();
    if uri != 1400 {
        // 8006 是在线人数等, 前端没有对应消息类型, 忽略。
        return;
    }

    // HYMessage: userInfo(struct, tag 0) / content(string, tag 3) /
    // bulletFormat(struct, tag 6)。
    let mut msg_reader = Reader::new(&msg);
    let user = msg_reader
        .read_struct(0)
        .map(|mut sender| sender.read_string(2))
        .unwrap_or_default();
    let content = msg_reader.read_string(3);
    let color_value = msg_reader
        .read_struct(6)
        .map(|mut format| format.read_int(0))
        .unwrap_or(0);
    let color = if color_value <= 0 {
        String::new()
    } else {
        format!("#{:06x}", color_value & 0xff_ffff)
    };
    if !user.is_empty() && !content.is_empty() {
        out.push(DanmakuMessage::chat(user, content, color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码 → 解码 round-trip: 模拟一条 uri=1400 弹幕推送的完整字节流。
    fn build_chat_frame(user: &str, content: &str, color: i64) -> Vec<u8> {
        let mut sender = Writer::new();
        sender.write_int(123, 0);
        sender.write_int(0, 1);
        sender.write_string(user, 2);
        sender.write_int(0, 3);

        let mut format = Writer::new();
        format.write_int(color, 0);

        let mut message = Writer::new();
        message.struct_begin(0);
        message.buf.extend_from_slice(&sender.buf);
        message.struct_end();
        message.write_string(content, 3);
        message.struct_begin(6);
        message.buf.extend_from_slice(&format.buf);
        message.struct_end();

        let mut push = Writer::new();
        push.write_int(0, 0);
        push.write_int(1400, 1);
        push.write_bytes(&message.buf, 2);
        push.write_int(0, 3);

        let mut frame = Writer::new();
        frame.write_int(7, 0);
        frame.write_bytes(&push.buf, 1);
        frame.buf
    }

    #[test]
    fn register_packet_decodes_back_to_same_fields() {
        let packet = encode_register(999_999_999_999, 12_345, 67_890);
        let mut outer = Reader::new(&packet);
        assert_eq!(outer.read_int(0), 1);
        let inner = outer.read_bytes(1).expect("register body");

        let mut reader = Reader::new(&inner);
        assert_eq!(reader.read_int(0), 999_999_999_999);
        assert_eq!(reader.read_int(1), 1);
        assert_eq!(reader.read_string(2), "");
        assert_eq!(reader.read_string(3), "");
        assert_eq!(reader.read_int(4), 12_345);
        assert_eq!(reader.read_int(5), 67_890);
        assert_eq!(reader.read_int(6), 0);
        assert_eq!(reader.read_int(7), 0);
    }

    #[test]
    fn decodes_chat_push_message() {
        let frame = build_chat_frame("观众甲", "主播好", 0xff8040);
        let mut out = Vec::new();
        decode_frame(&frame, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "chat");
        assert_eq!(out[0].user, "观众甲");
        assert_eq!(out[0].content, "主播好");
        assert_eq!(out[0].color, "#ff8040");
    }

    #[test]
    fn non_positive_color_falls_back_to_theme_color() {
        let frame = build_chat_frame("乙", "无色弹幕", -1);
        let mut out = Vec::new();
        decode_frame(&frame, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].color, "");
    }

    #[test]
    fn ignores_non_push_and_other_uris() {
        // cmd = 1(注册回应)不产出消息。
        let mut frame = Writer::new();
        frame.write_int(1, 0);
        let mut out = Vec::new();
        decode_frame(&frame.buf, &mut out);
        assert!(out.is_empty());

        // uri = 8006(在线人数)忽略。
        let mut push = Writer::new();
        push.write_int(0, 0);
        push.write_int(8006, 1);
        let mut online = Writer::new();
        online.write_int(1234, 0);
        push.write_bytes(&online.buf, 2);
        let mut frame = Writer::new();
        frame.write_int(7, 0);
        frame.write_bytes(&push.buf, 1);
        decode_frame(&frame.buf, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn truncated_or_garbage_frames_do_not_panic() {
        let frame = build_chat_frame("甲", "内容", 0);
        let mut out = Vec::new();
        for cut in [1, 3, 7, frame.len() / 2, frame.len() - 1] {
            decode_frame(&frame[..cut], &mut out);
        }
        decode_frame(&[0xff, 0xff, 0xff], &mut out);
    }

    #[test]
    fn finds_ids_in_room_page_html() {
        let html = r#"<script>window.HNF_GLOBAL_INIT = {"roomInfo":{"tLiveInfo":{"lYyid":1385123456}},"x":{"lChannelId":987654,"lSubChannelId":987653}};</script>"#;
        assert_eq!(
            find_id(html, r#""lYyid"\s*:\s*(\d+)"#),
            Some(1_385_123_456)
        );
        assert_eq!(find_id(html, r#""lChannelId"\s*:\s*(\d+)"#), Some(987_654));
        assert_eq!(find_id(html, r#""lSubChannelId"\s*:\s*(\d+)"#), Some(987_653));
        assert_eq!(find_id(html, r#""lNotExist"\s*:\s*(\d+)"#), None);
    }

    /// 真实链路自检: 抓房间页 → 连 WS → 注册 → 读若干帧。
    /// 需要外网, 默认忽略(`cargo test --lib -- --ignored danmaku`)。
    #[tokio::test]
    #[ignore]
    async fn connects_and_reads_a_real_room() {
        let http = HttpClient::new(20, None).expect("http");
        // 高人气在播房间, 保证短时间内一定有弹幕(未开播房间页没有弹幕参数)。
        let room_id = "691406";
        let args = fetch_args(&http, room_id).await.expect("danmaku args");
        println!("yyid={} top_sid={} sub_sid={}", args.yyid, args.top_sid, args.sub_sid);

        let (mut stream, _) = tokio_tungstenite::connect_async(SERVER_URL)
            .await
            .expect("ws connect");
        stream
            .send(Message::Binary(encode_register(
                args.yyid, args.top_sid, args.sub_sid,
            )))
            .await
            .expect("send register");

        let mut frames = 0;
        let mut messages = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        while frames < 20 && tokio::time::Instant::now() < deadline {
            let frame = tokio::time::timeout(Duration::from_secs(10), stream.next()).await;
            let Ok(Some(Ok(frame))) = frame else { break };
            frames += 1;
            match frame {
                Message::Binary(bytes) => {
                    let mut probe = Reader::new(&bytes);
                    let cmd = probe.read_int(0);
                    let uri = if cmd == 7 {
                        probe
                            .read_bytes(1)
                            .map(|push| Reader::new(&push).read_int(1))
                            .unwrap_or(-1)
                    } else {
                        -1
                    };
                    println!("frame {frames}: {} bytes cmd={cmd} uri={uri}", bytes.len());
                    decode_frame(&bytes, &mut messages);
                }
                other => println!("frame {frames}: {other:?}"),
            }
        }
        println!("frames={frames} messages={}", messages.len());
        for message in messages.iter().take(5) {
            println!("  {}: {} {}", message.kind, message.user, message.content);
        }
        assert!(frames > 0, "should receive at least one frame");
        assert!(!messages.is_empty(), "should decode at least one danmaku");
    }
}
