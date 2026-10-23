use super::*;

/// In-process LRU so re-rendered covers do not refetch. WebView2 does not
/// cache custom-scheme responses.
pub(super) const COVER_CACHE_CAP: usize = 200;

pub(super) fn cover_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, (Vec<u8>, String)>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, (Vec<u8>, String)>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Drop one URL from the in-process cover cache. Disk cleanup alone is not
/// enough: the current process would otherwise keep serving the deleted bytes
/// until the 200-entry cap clears the whole map.
pub(crate) fn remove_cover_cache_url(url: &str) {
    if let Ok(mut cache) = cover_cache().lock() {
        cache.remove(url);
    }
}

pub(super) async fn cover_route(state: &AppState, query: &str) -> tauri::http::Response<Vec<u8>> {
    let params: HashMap<String, String> =
        serde_urlencoded::from_str(query).unwrap_or_default();
    let url = params.get("path").map(String::as_str).unwrap_or_default();
    if url.is_empty() {
        return http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default();
    }

    let cache_key = url.to_string();
    if let Some((bytes, content_type)) = cover_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&cache_key).cloned())
    {
        return bytes_response(bytes, &content_type, Some("86400"));
    }

    // 本地 PDF 封面: /local-pdf/<hash>/cover.jpg → storage 下的书目录
    if let Some(rest) = url.strip_prefix("/local-pdf/") {
        return local_pdf_cover_route(state, rest).await;
    }

    // "public" namespace matches the original unauthenticated cover endpoint.
    match state.book_service.get_cover("public", url).await {
        Ok((bytes, content_type)) => {
            if let Ok(mut cache) = cover_cache().lock() {
                if cache.len() >= COVER_CACHE_CAP {
                    cache.clear();
                }
                cache.insert(cache_key, (bytes.clone(), content_type.clone()));
            }
            bytes_response(bytes, &content_type, Some("86400"))
        }
        Err(_) => http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default(),
    }
}

/// 本地 PDF 封面: rest = "<hash>/cover.<ext>"。hash 在 service 侧校验
/// (32 位 hex), 文件名只接受三个固定值, 无路径穿越面。
pub(super) async fn local_pdf_cover_route(
    state: &AppState,
    rest: &str,
) -> tauri::http::Response<Vec<u8>> {
    let Some((hash, file_name)) = rest.split_once('/') else {
        return http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default();
    };
    if !matches!(file_name, "cover.jpg" | "cover.png" | "cover.jp2") {
        return http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default();
    }
    match state
        .local_pdf_book_service
        .read_cover_file("default", hash, file_name)
        .await
    {
        Ok((bytes, content_type)) => {
            bytes_response(bytes, &content_type, Some("private, max-age=86400"))
        }
        Err(_) => http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default(),
    }
}

/// 本地 PDF 原始文件: `/pdf?bookUrl=local-pdf:<hash>`。
///
/// 前端 pdf.js 整本取走自己渲染 (与 Legado 的 PdfRenderer 同一分工: 后端只
/// 管文件, 页面光栅化在前端)。bookUrl 在 service 侧校验 32 位 hex, 无路径穿越面。
pub(super) async fn pdf_route(state: &AppState, query: &str) -> tauri::http::Response<Vec<u8>> {
    let params: HashMap<String, String> =
        serde_urlencoded::from_str(query).unwrap_or_default();
    let book_url = params.get("bookUrl").map(String::as_str).unwrap_or_default();
    if book_url.is_empty() {
        return http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default();
    }
    match state
        .local_pdf_book_service
        .read_pdf_file("default", book_url)
        .await
    {
        Ok(bytes) => bytes_response(bytes, "application/pdf", Some("private, max-age=3600")),
        Err(_) => http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default(),
    }
}

pub(super) async fn epub_route(state: &AppState, query: &str) -> tauri::http::Response<Vec<u8>> {
    let params: HashMap<String, String> =
        serde_urlencoded::from_str(query).unwrap_or_default();
    let book_url = params.get("bookUrl").map(String::as_str).unwrap_or_default();
    let path = params.get("path").map(String::as_str).unwrap_or_default();
    if book_url.is_empty() || path.is_empty() {
        return http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default();
    }
    match state
        .local_epub_book_service
        .get_asset("default", book_url, path)
        .await
    {
        Ok(asset) => bytes_response(asset.bytes, &asset.content_type, Some("private, max-age=3600")),
        Err(_) => http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default(),
    }
}

/// Serves user-uploaded files from `storage/assets/<ns>/<type>/<name>`.
/// The path query is the asset URL minus the leading slash, e.g.
/// `files?path=default/ai-maps/map.png`.
pub(super) async fn files_route(state: &AppState, query: &str) -> tauri::http::Response<Vec<u8>> {
    let params: HashMap<String, String> =
        serde_urlencoded::from_str(query).unwrap_or_default();
    let Some(rel) = params.get("path") else {
        return http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default();
    };
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() || rel.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        return http::Response::builder()
            .status(400)
            .body(Vec::new())
            .unwrap_or_default();
    }
    let full = std::path::PathBuf::from(&state.config.storage_dir)
        .join("assets")
        .join(rel);
    match tokio::fs::read(&full).await {
        Ok(bytes) => {
            let content_type = mime_from_ext(&full).to_string();
            bytes_response(bytes, &content_type, None)
        }
        Err(_) => http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default(),
    }
}

pub(super) fn bytes_response(
    bytes: Vec<u8>,
    content_type: &str,
    cache_control: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    let mut builder = http::Response::builder().status(200);
    if let Ok(value) = http::HeaderValue::from_str(content_type) {
        builder = builder.header(http::header::CONTENT_TYPE, value);
    }
    if let Some(cc) = cache_control {
        if let Ok(value) = http::HeaderValue::from_str(cc) {
            builder = builder.header(http::header::CACHE_CONTROL, value);
        }
    }
    builder
        .header("access-control-allow-origin", "*")
        .header("cross-origin-resource-policy", "cross-origin")
        .body(bytes)
        .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

pub(super) fn mime_from_ext(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "avif" => "image/avif",
        "json" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

// ─────────────────────────── bookSourceProxy ───────────────────────────
