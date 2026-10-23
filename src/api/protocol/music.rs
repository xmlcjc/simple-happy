use super::*;

/// 音乐取流: `/musicStream?u=<url>&h=<urlencoded JSON headers>`。
pub(super) async fn music_stream_route(state: &AppState, query: &str) -> tauri::http::Response<Vec<u8>> {
    let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let url = params.get("u").map(String::as_str).unwrap_or("");
    let headers = params
        .get("h")
        .and_then(|raw| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw).ok())
        .unwrap_or_default();
    match state.music_service.stream(url, &headers).await {
        Ok(stream) => {
            let mut builder = http::Response::builder()
                .status(stream.status)
                .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .header(http::header::CACHE_CONTROL, "no-store");
            if let Some(content_type) = stream.content_type {
                builder = builder.header(http::header::CONTENT_TYPE, content_type);
            }
            builder
                .body(stream.bytes)
                .unwrap_or_else(|_| error_response(500, "build music stream response failed"))
        }
        Err(err) => error_response(502, &err.to_string()),
    }
}

/// 本地音乐播放: `/musicLocal?path=<绝对路径>`。
pub(super) async fn music_local_route(state: &AppState, query: &str) -> tauri::http::Response<Vec<u8>> {
    let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let Some(path) = params.get("path") else {
        return error_response(400, "missing path");
    };
    match state.music_service.local().find("default", path).await {
        Ok(Some(track)) => match tokio::fs::read(&track.path).await {
            Ok(bytes) => bytes_response(
                bytes,
                crate::service::music::local::audio_mime(&track.path),
                None,
            ),
            Err(_) => error_response(404, "local audio not found"),
        },
        Ok(None) => error_response(403, "local audio is not registered"),
        Err(err) => error_response(500, &err.to_string()),
    }
}
