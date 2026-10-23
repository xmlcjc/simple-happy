use super::*;

/// 登录页代理: 从路径里取会话, 把**剩余路径**原样转发给上游。
///
/// 会话放在路径里而不是 cookie/Referer, 因为 WebView2 对自定义协议的子资源
/// 请求既不附 Cookie 也不附 Referer(实测), 只有 URL 本身一定会被带上。
pub(super) async fn source_login_proxy_route(
    state: &AppState,
    path: &str,
    query: &str,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let rest = match path.strip_prefix(SESSION_PATH_PREFIX) {
        Some(rest) => rest,
        None => return error_response(400, "invalid login path"),
    };
    let (session, upstream_path) = match rest.split_once('/') {
        Some((session, upstream_path)) => (session, format!("/{upstream_path}")),
        None => (rest, "/".to_string()),
    };
    let login_session = match urlencoding::decode(session) {
        Ok(value) => value.into_owned(),
        Err(_) => return error_response(400, "invalid login session"),
    };
    if login_session.is_empty() {
        return error_response(403, "loginSession required");
    }
    let Some(upstream_origin) = source_login_upstream_origin(state, &login_session).await else {
        return error_response(403, "invalid or expired loginSession");
    };
    let mut target = match Url::parse(&upstream_origin) {
        Ok(url) => url,
        Err(_) => return error_response(400, "invalid upstream origin"),
    };
    target.set_path(&upstream_path);
    if !query.is_empty() {
        target.set_query(Some(query));
    }
    book_source_proxy_forward(state, &login_session, &target.to_string(), request).await
}

/// 按登录会话反查上游 origin。
pub(super) async fn source_login_upstream_origin(state: &AppState, login_session: &str) -> Option<String> {
    state
        .book_service
        .source_for_login_session(login_session)
        .await
        .and_then(|source_url| Url::parse(&normalize_source_url(&source_url)).ok())
        .map(|url| url.origin().ascii_serialization())
}

/// 首次导航用的会话参数名: 兜底路由据此认出「这是登录页」并从 query 取会话。
pub(super) const SESSION_QUERY_KEY: &str = "__readerLoginSession";

/// 登录页的兜底代理: 把 reader origin 上「属于某个登录页」的请求转发回上游。
///
/// 判定会话的来源, 按可靠性排序:
/// 1. 首次导航显式带上的 `__readerLoginSession`(此时还没有 cookie);
/// 2. 登录代理页写下的同源 cookie(子资源请求只能靠它);
/// 3. 登录页的 `Referer`(兜底)。
///
/// 三条都不成立时返回 `None`, 交回 404 —— 否则这个 origin 会变成任意转发器。
pub(super) async fn source_login_fallback_route(
    state: &AppState,
    path: &str,
    query: &str,
    request: &tauri::http::Request<Vec<u8>>,
) -> Option<tauri::http::Response<Vec<u8>>> {
    let mut params: HashMap<String, String> =
        serde_urlencoded::from_str(query).unwrap_or_default();
    let explicit_session = params.remove(SESSION_QUERY_KEY);

    let (login_session, upstream_origin) = match explicit_session {
        Some(session) => {
            let origin = state
                .book_service
                .source_for_login_session(&session)
                .await
                .and_then(|source_url| Url::parse(&normalize_source_url(&source_url)).ok())
                .map(|url| url.origin().ascii_serialization())?;
            (session, origin)
        }
        None => match parse_session_cookie(request.headers()) {
            Some(pair) => pair,
            None => {
                let referer = request
                    .headers()
                    .get(http::header::REFERER)
                    .and_then(|value| value.to_str().ok())?;
                let referer_url = Url::parse(referer).ok()?;
                if referer_url.origin().ascii_serialization() != reader_scheme_origin() {
                    return None;
                }
                let referer_query: HashMap<String, String> =
                    serde_urlencoded::from_str(referer_url.query()?).unwrap_or_default();
                let session = referer_query.get("loginSession")?.clone();
                let origin = match referer_query.get("__readerUpstreamOrigin") {
                    Some(origin) => origin.clone(),
                    None => {
                        let target = referer_query.get("url")?;
                        Url::parse(target).ok()?.origin().ascii_serialization()
                    }
                };
                (session, origin)
            }
        },
    };

    let mut target = Url::parse(&upstream_origin).ok()?;
    target.set_path(path);
    // 剥掉传输参数: 上游页面不该看到它们。
    if !params.is_empty() {
        let rebuilt: Vec<(String, String)> = params.into_iter().collect();
        target.set_query(Some(&serde_urlencoded::to_string(&rebuilt).ok()?));
    }
    let target_url = target.to_string();
    Some(book_source_proxy_forward(state, &login_session, &target_url, request).await)
}

/// 判断响应体是否 JavaScript(打包产物的主脚本/分片)。
pub(super) fn is_script_response(content_type: Option<&str>, body: &[u8]) -> bool {
    let _ = body;
    let Some(content_type) = content_type else {
        return false;
    };
    let lowered = content_type.to_ascii_lowercase();
    lowered.contains("javascript") || lowered.contains("ecmascript")
}

/// 构建脚本响应里的会话路径前缀: `/__login/<session>/`。
pub(super) fn script_session_prefix(login_session: &str) -> String {
    format!("/__login/{}/", urlencoding::encode(login_session))
}

/// 给打包产物里**写死的分片前缀**补上会话段。
///
/// Vite 把资源基准烘进代码: `Ab=e=>"/user/"+e`(路由 base `/user/`)。分片地址
/// 因此完全不经过文档基准, 无论 iframe 挂在哪个路径下都会请求 `/user/...` ——
/// 那里没有会话段, 后端无从判断该转发到哪, 只能 404。
///
/// **只**改写「整个字符串就是一个前导斜杠 + 单段 + 尾随斜杠」的字面量
/// (`"/user/"`)。业务接口地址(`"/site/info"`)必须原样保留: 它们会和 axios 的
/// baseURL(`/api`)再拼接, 插了会话段就变成上游不存在的
/// `/api/__login/.../site/info`。
pub(super) fn annotate_script_asset_prefix(script: &str, login_session: &str) -> String {
    if login_session.is_empty() {
        return script.to_string();
    }
    let prefix = script_session_prefix(login_session);
    // `regex` crate 不支持反向引用, 两种引号各写一条。
    let pattern = Regex::new(r#""/([A-Za-z0-9_.~%-]+)/""#).expect("valid double-quoted prefix");
    let script = pattern
        .replace_all(script, |caps: &Captures| {
            let segment = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            if segment == "__login" {
                return caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();
            }
            format!("\"{prefix}{segment}/\"")
        })
        .into_owned();
    let pattern = Regex::new(r"'/([A-Za-z0-9_.~%-]+)/'").expect("valid single-quoted prefix");
    pattern
        .replace_all(&script, |caps: &Captures| {
            let segment = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            if segment == "__login" {
                return caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();
            }
            format!("'{prefix}{segment}/'")
        })
        .into_owned()
}

/// 只改写 HTML 标签里的资源属性(`src|href|action="…"`), 保留上游路径并补上会话段。
///
/// 页面里写死的绝对路径(`/user/assets/index.js`)不继承文档路径, 不补会话段就
/// 会落到 reader origin 根下 404。相对路径与协议相对地址保持原样。
pub(super) fn rewrite_login_html_resource_attrs(html: &str, login_session: &str) -> String {
    if login_session.is_empty() {
        return html.to_string();
    }
    let prefix = script_session_prefix(login_session);
    let pattern = Regex::new(r#"(?i)\b(src|href|action)\s*=\s*(?:"([^"]*)"|'([^']*)')"#)
        .expect("valid resource attribute regex");
    pattern
        .replace_all(html, |caps: &Captures| {
            let whole = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            let attr = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let double = caps.get(2);
            let value = double
                .or_else(|| caps.get(3))
                .map(|m| m.as_str())
                .unwrap_or_default();
            let quote = if double.is_some() { '"' } else { '\'' };
            if !value.starts_with('/')
                || value.starts_with("//")
                || value.starts_with(SESSION_PATH_PREFIX)
            {
                return whole.to_string();
            }
            let trimmed = value.trim_start_matches('/');
            if trimmed.is_empty() {
                return whole.to_string();
            }
            format!("{attr}={quote}{prefix}{trimmed}{quote}")
        })
        .into_owned()
}

/// 从请求头里取 `reader_login_session` cookie, 解出 `(loginSession, 上游origin)`。
pub(super) fn parse_session_cookie(headers: &http::HeaderMap) -> Option<(String, String)> {
    let raw = headers
        .get(http::header::COOKIE)
        .and_then(|value| value.to_str().ok())?;
    for part in raw.split(';') {
        let part = part.trim();
        let Some(value) = part.strip_prefix("reader_login_session=") else {
            continue;
        };
        let decoded = urlencoding::decode(value).ok()?;
        let (session, origin) = decoded.split_once('|')?;
        if session.is_empty() || origin.is_empty() {
            return None;
        }
        return Some((session.to_string(), origin.to_string()));
    }
    None
}

// ─────────────────────────── 书源面板 (showBrowser) ───────────────────────────
