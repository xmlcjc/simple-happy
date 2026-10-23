use super::*;

pub(super) fn error_response(status: u16, message: &str) -> tauri::http::Response<Vec<u8>> {
    let body = serde_json::to_vec(&ApiResponse::<serde_json::Value>::err(message))
        .unwrap_or_default();
    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(body)
        .unwrap_or_default()
}

pub(super) fn resolve_proxy_target_url(
    raw_target_url: &str,
    book_source_url: &str,
) -> Result<String, AppError> {
    let repaired = repair_encoded_url(raw_target_url);
    if let Ok(url) = Url::parse(&repaired) {
        return Ok(url.to_string());
    }

    let base = normalize_source_url(book_source_url);
    let base = Url::parse(&base)
        .map_err(|e| AppError::BadRequest(format!("invalid bookSourceUrl: {}", e)))?;
    base.join(&repaired)
        .map(|u| u.to_string())
        .map_err(|e| AppError::BadRequest(format!("invalid proxy target url: {}", e)))
}

/// 面板页写入 cookie 时, 把目标地址解析成可写入 cookie jar 的绝对 URL。
///
/// 面板里的地址可能是相对的(上游页在子路径下), 也可能带 `__readerPanelUrl`
/// 这类代理参数, 因此统一按代理目标地址的规则解析: 先尝试直接当作绝对 URL,
/// 失败再以书源地址为基准拼接。
pub(crate) fn resolve_panel_cookie_url(
    raw_target_url: &str,
    book_source_url: &str,
) -> Result<Url, AppError> {
    let repaired = repair_encoded_url(raw_target_url);
    if let Ok(url) = Url::parse(&repaired) {
        return Ok(url);
    }
    let base = normalize_source_url(book_source_url);
    let base = Url::parse(&base)
        .map_err(|e| AppError::BadRequest(format!("invalid bookSourceUrl: {}", e)))?;
    base.join(&repaired)
        .map_err(|e| AppError::BadRequest(format!("invalid cookie target url: {}", e)))
}

/// Login pages are rendered through the local URI scheme. Restrict every
/// proxied target to the source's host family so a valid login session cannot
/// be reused as a cross-domain request or SSRF primitive.
pub(crate) fn proxy_target_matches_source(target_url: &str, source_url: &str) -> bool {
    let Ok(target) = Url::parse(target_url) else {
        return false;
    };
    let Ok(source) = Url::parse(&normalize_source_url(source_url)) else {
        return false;
    };
    if !matches!(target.scheme(), "http" | "https")
        || !matches!(source.scheme(), "http" | "https")
    {
        return false;
    }
    let Some(target_host) = target.host_str() else {
        return false;
    };
    let Some(source_host) = source.host_str() else {
        return false;
    };
    let target_host = target_host.trim_end_matches('.').to_ascii_lowercase();
    let source_host = source_host.trim_end_matches('.').to_ascii_lowercase();
    match (source.port(), target.port()) {
        (Some(source_port), Some(target_port)) if source_port != target_port => return false,
        (Some(source_port), None) if target.port_or_known_default() != Some(source_port) => {
            return false
        }
        (None, Some(target_port)) if !matches!(target_port, 80 | 443) => return false,
        _ => {}
    }
    if target_host == source_host {
        return true;
    }

    // IP addresses and local development hosts have no meaningful parent
    // domain, so only the exact host is allowed for them.
    if target_host.parse::<std::net::IpAddr>().is_ok()
        || source_host.parse::<std::net::IpAddr>().is_ok()
        || source_host == "localhost"
        || source_host.ends_with(".localhost")
        || target_host == "localhost"
        || target_host.ends_with(".localhost")
    {
        return false;
    }

    let target_root = registrable_domain_from_host(&target_host);
    let source_root = registrable_domain_from_host(&source_host);
    !target_root.is_empty() && target_root == source_root
}

pub(super) fn extract_upstream_referer(headers: &http::HeaderMap) -> Option<String> {
    let raw = headers.get(http::header::REFERER)?.to_str().ok()?;
    let referer = Url::parse(raw).ok()?;
    let params: HashMap<String, String> = referer.query_pairs().into_owned().collect();
    params.get("url").cloned().map(|v| repair_encoded_url(&v))
}

pub(super) async fn forward_book_source_request(
    book_service: &crate::service::book_service::BookService,
    source: &BookSource,
    login_session: &str,
    method: &http::Method,
    headers: &http::HeaderMap,
    target_url: &str,
    upstream_referer: Option<&str>,
    body: &[u8],
) -> Result<tauri::http::Response<Vec<u8>>, AppError> {
    let client = book_service
        .source_http_client("default", &source.book_source_url, None)
        .await?;
    let req_method = match *method {
        http::Method::GET => reqwest::Method::GET,
        http::Method::POST => reqwest::Method::POST,
        _ => return Err(AppError::BadRequest("unsupported proxy method".to_string())),
    };

    let header_source = source.clone();
    let header_service = book_service.clone();
    let header_spec = tokio::task::spawn_blocking(move || {
        header_service.source_scope("default", &header_source, || {
            crate::crawler::url_analyzer::source_header_spec(&header_source)
        })?
    }).await.map_err(|err| AppError::Internal(err.into()))??;
    let mut forwarded = merge_proxy_headers(header_spec.headers, headers)?;
    if !forwarded.contains_key(http::header::REFERER) {
        forwarded.insert(http::header::REFERER, http::HeaderValue::from_str(upstream_referer.unwrap_or(target_url))
            .map_err(|err| AppError::BadRequest(format!("网页来源地址无效: {err}")))?);
    }
    let referer_value = forwarded.get(http::header::REFERER).and_then(|value| value.to_str().ok()).unwrap_or(target_url).to_string();
    if !forwarded.contains_key(http::header::ORIGIN) {
        if let Ok(url) = Url::parse(&referer_value) {
            if let Ok(origin) = http::HeaderValue::from_str(&url.origin().ascii_serialization()) {
                forwarded.insert(http::header::ORIGIN, origin);
            }
        }
    }
    if *method == http::Method::POST && !forwarded.contains_key(http::header::CONTENT_TYPE) {
        forwarded.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/x-www-form-urlencoded; charset=UTF-8"),
        );
    }
    if is_ajax_api_target(target_url) && !forwarded.contains_key("x-requested-with") {
        forwarded.insert("x-requested-with", http::HeaderValue::from_static("XMLHttpRequest"));
    }
    let mut builder = client.request(req_method, target_url).headers(forwarded);
    if *method == http::Method::POST {
        builder = builder.body(body.to_vec());
    }

    tracing::info!(
        "bookSourceProxy upstream request: method={} target={} referer={} body_len={}",
        method,
        target_url,
        referer_value,
        body.len()
    );
    let upstream = builder.send().await.map_err(AppError::Http)?;
    let status = upstream.status();
    let final_url = upstream.url().to_string();
    let content_type = upstream
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = upstream.bytes().await.map_err(AppError::Http)?;
    tracing::info!(
        "bookSourceProxy upstream response: method={} target={} status={} final_url={}",
        method,
        target_url,
        status,
        final_url
    );
    if status.is_client_error() || status.is_server_error() {
        let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(400)]).replace('\n', "\\n");
        tracing::warn!(
            "bookSourceProxy upstream error body: target={} status={} preview={}",
            target_url,
            status,
            preview
        );
    }

    let html_response = is_html_response(content_type.as_deref(), &bytes);
    let mut response_builder = http::Response::builder().status(status.as_u16());
    if let Some(ct) = if html_response { Some("text/html; charset=utf-8") } else { content_type.as_deref() } {
        if let Ok(value) = http::HeaderValue::from_str(ct) {
            response_builder = response_builder.header(http::header::CONTENT_TYPE, value);
        }
    }
    // Keep upstream cookies inside the source-scoped client. Never expose
    // them to the shared reader.localhost browser origin.
    //
    // 这里**不能**下发 `Clear-Site-Data: "cookies"`: 该指令会被浏览器执行并
    // 清空整个 origin 的 Cookie, 而它挂在每一个代理响应上(页面、JS、CSS、图标),
    // 单页登录页一次加载就是几十个请求 —— 等于反复清空登录页自己刚写的 Cookie,
    // 页面因此表现为空白/反复重载。目标站点的 Cookie 本来就只存在后端按书源
    // 隔离的 jar 里, 浏览器侧不需要清。
    response_builder = response_builder.header(http::header::CACHE_CONTROL, "no-store");
    // 登录页里的 `<script type="module" crossorigin src=...>` 按 CORS 规则加载:
    // 模块脚本与任何带 `crossorigin` 的标签, 浏览器都按 CORS 模式校验响应头,
    // 不看"是否同源"。缺 `Access-Control-Allow-Origin` 时脚本会被静默拒绝
    // 执行, 表现为登录页一片空白 —— 单页应用(Vue/React)的主脚本正是这种写法。
    //
    // 带 Cookie 的请求(credentials: include)还要求 `Allow-Credentials: true`,
    // 且此时 `Allow-Origin` 不能是 `*`, 必须回显实际 Origin。
    if let Some(origin) = headers.get(http::header::ORIGIN) {
        response_builder = response_builder
            .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.clone())
            .header(http::header::VARY, "Origin")
            .header(http::header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true");
    } else {
        response_builder =
            response_builder.header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
    }

    let body = if html_response {
        // 登录 iframe 跑在 `/__login/<session>/` 路径空间下(见前端 frameUrl),
        // 会话因此随每个子资源请求一起到达 —— WebView2 对自定义协议的子资源
        // 既不附 Cookie 也不附 Referer(实测), 只有 URL 一定会被带上。
        //
        // 上游路径必须完整保留: 打包产出的单页应用把路径写死进代码(Vue Router
        // 的 base `/user/`、分片 `"/user/" + chunk`), 路径一旦对不上就全部 404,
        // 应用永远挂载不起来 —— 纯白页。
        let text = crate::crawler::fetcher::decode_body(&bytes, None, content_type.as_deref());
        // 只改写 HTML 标签里的**绝对路径属性**(`src="/user/assets/x.js"`)。
        // 不能改脚本字符串: 运行时拼接的地址(`Pe("/site/info")` 配 axios 的
        // baseURL `/api`)会被插坏, 变成 `/api/__login/.../site/info`。
        let text = rewrite_login_html_resource_attrs(&text, login_session);
        let harvest = build_cookie_harvest_script(&source.book_source_url, login_session);
        let injected = if text.contains("</head>") {
            text.replace("</head>", &format!("{harvest}</head>"))
        } else if text.contains("</body>") {
            text.replace("</body>", &format!("{harvest}</body>"))
        } else {
            format!("{harvest}{text}")
        };
        injected.into_bytes()
    } else if is_script_response(content_type.as_deref(), &bytes) {
        // 打包产物里的资源基准(`Ab=e=>"/user/"+e`)是**字符串拼接**出来的分片
        // 前缀, 不经过文档基准。只改这种"整段等于 `/字面量/`"的形态, 不动
        // `"/site/info"` 这类业务接口地址(它们会与 baseURL 再拼接)。
        let text = crate::crawler::fetcher::decode_body(&bytes, None, content_type.as_deref());
        annotate_script_asset_prefix(&text, login_session).into_bytes()
    } else {
        bytes.to_vec()
    };

    Ok(response_builder
        .body(body)
        .unwrap_or_else(|_| http::Response::new(Vec::new())))
}


// Source settings override the embedded browser's defaults. HeaderMap::insert
// replaces case-insensitive matches so Referer/User-Agent are sent exactly once.

pub(super) fn merge_proxy_headers(
    source_headers: Vec<(String, String)>,
    browser_headers: &http::HeaderMap,
) -> Result<http::HeaderMap, AppError> {
    let mut merged = http::HeaderMap::new();
    for (name, value) in browser_headers {
        if should_forward_request_header(name.as_str()) {
            merged.insert(name.clone(), value.clone());
        }
    }
    for (name, value) in source_headers {
        let name = http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|err| AppError::BadRequest(format!("书源请求头名称无效: {err}")))?;
        let value = http::HeaderValue::from_str(&value)
            .map_err(|err| AppError::BadRequest(format!("书源请求头值无效: {err}")))?;
        merged.insert(name, value);
    }
    // The browser's boundary/encoding describes the actual submitted bytes.
    if let Some(value) = browser_headers.get(http::header::CONTENT_TYPE) {
        merged.insert(http::header::CONTENT_TYPE, value.clone());
    }
    Ok(merged)
}

/// 按请求体的真实形态校正表单 `Content-Type`。
///
/// WebView2 把自定义协议请求交给宿主时可能不提供 `Content-Type`, 也可能给一个与
/// 实际请求体不符的头。而后端(PHP 等)在缺该头、或头与体不匹配时都**完全不解析
/// 请求体**: `$_POST` 为空, 表现为「后端收不到任何字段」(某聚合源的注册接口即回
/// 「无效的操作」)。此时也不能一律按 urlencoded 兜底 —— multipart 体被错标后同样
/// 解析不出字段, 症状不变。因此以体为准:
/// - 体的首行是 `--boundary` -> 声明为该 boundary 的 multipart 体;
///   若已有头描述的就是这份体(boundary 一致), 原样保留浏览器给的头。
/// - 否则仅在缺头时按 urlencoded 兜底, 不覆盖已有的非 multipart 头。
pub(super) fn inherit_form_content_type(headers: &mut http::HeaderMap, body: &[u8]) {
    let declared = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if let Some(boundary) = multipart_boundary(body) {
        let describes_body = declared.to_ascii_lowercase().starts_with("multipart/form-data")
            && declared.contains(&boundary);
        if describes_body {
            return;
        }
        if let Ok(value) =
            http::HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
        {
            headers.insert(http::header::CONTENT_TYPE, value);
        }
        return;
    }
    if headers.contains_key(http::header::CONTENT_TYPE) {
        return;
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/x-www-form-urlencoded; charset=UTF-8"),
    );
}

/// 取 multipart 体的首行 boundary(不含前导 `--`)。非 multipart 返回 `None`。
pub(super) fn multipart_boundary(body: &[u8]) -> Option<String> {
    let rest = body.strip_prefix(b"--")?;
    let end = rest.iter().position(|&b| b == b'\r' || b == b'\n')?;
    let boundary = &rest[..end];
    // RFC 2046: boundary 为 1..=70 个 bchars, 不含空白与控制字符。
    if boundary.is_empty() || boundary.len() > 70 || !boundary.is_ascii() {
        return None;
    }
    if boundary
        .iter()
        .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return None;
    }
    String::from_utf8(boundary.to_vec()).ok()
}

pub(super) fn should_forward_request_header(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "content-length"
            | "authorization"
            | "cookie"
            | "referer"
            | "origin"
            | "connection"
    )
}

pub(super) fn is_ajax_api_target(target_url: &str) -> bool {
    if let Ok(url) = Url::parse(target_url) {
        let path = url.path().to_ascii_lowercase();
        return path.ends_with("_api") || path.contains("/api/");
    }
    false
}

pub(super) fn is_html_response(content_type: Option<&str>, body: &[u8]) -> bool {
    if let Some(ct) = content_type {
        if ct.to_ascii_lowercase().contains("text/html") {
            return true;
        }
    }
    // 避免 String 分配: 在原始 bytes 上做大小写不敏感匹配。
    // HTML 前缀是 ASCII, 多字节 UTF-8 的首字节 >= 0x80 不会与 "<html" 混淆。
    let prefix = &body[..body.len().min(256)];
    prefix
        .windows(5)
        .any(|w| w.eq_ignore_ascii_case(b"<html"))
        || prefix
            .windows(14)
            .any(|w| w.eq_ignore_ascii_case(b"<!doctype html"))
}
