//! 音频取流代理。
//!
//! 部分插件的播放地址要求 Referer/UA/Cookie, 浏览器里 `<audio src>` 无法自定义
//! 这些头。前端改为请求 `reader://musicStream?u=...&h=...`, 由这里带上头部
//! 取回整块字节, 再在页面内转成 Blob URL 播放。
//!
//! 限制: 单曲 64MB。超过上限直接报错, 避免一个异常地址把内存打满。

use serde_json::{Map, Value};

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;

const MAX_AUDIO_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub struct AudioStream {
    pub status: u16,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

pub async fn fetch_audio(
    http: &HttpClient,
    url: &str,
    headers: &Map<String, Value>,
) -> Result<AudioStream, AppError> {
    let url = url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::BadRequest(
            "播放地址必须是 http/https 链接".to_string(),
        ));
    }

    let mut request = http.client().get(url);
    for (key, value) in headers {
        let text = value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string());
        request = request.header(key.as_str(), text);
    }

    let response = request.send().await?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(AppError::BadRequest(format!(
            "音频源返回 HTTP {status}，该歌曲可能需要登录或无版权"
        )));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = response.bytes().await?;
    if bytes.len() > MAX_AUDIO_BYTES {
        return Err(AppError::BadRequest(format!(
            "音频文件过大（{} MB），已超过 {} MB 上限",
            bytes.len() / 1024 / 1024,
            MAX_AUDIO_BYTES / 1024 / 1024
        )));
    }
    Ok(AudioStream {
        status,
        content_type,
        bytes: bytes.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_non_http_urls() {
        let http = HttpClient::new(3, None).unwrap();
        let err = fetch_audio(&http, "file:///etc/passwd", &Map::new())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("http"));
    }
}
