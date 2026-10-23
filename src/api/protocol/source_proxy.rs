use super::*;

#[derive(Debug, Deserialize, Default)]
pub(super) struct BookSourceProxyParam {
    #[serde(rename = "loginSession")]
    login_session: Option<String>,
    url: Option<String>,
}

pub(super) async fn book_source_proxy_route(
    state: &AppState,
    query: &str,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let q: BookSourceProxyParam = serde_urlencoded::from_str(query).unwrap_or_default();
    let Some(login_session) = q.login_session else {
        return error_response(403, "loginSession required");
    };
    let Some(raw_target_url) = q.url else {
        return error_response(400, "url required");
    };
    book_source_proxy_forward(state, &login_session, &raw_target_url, request).await
}

/// 会话校验 + 同源校验 + 实际转发。两种登录代理入口共用:
/// 查询串式(`/bookSourceProxy?url=…`)与兜底路由(按上游原始路径转发)。
pub(super) async fn book_source_proxy_forward(
    state: &AppState,
    login_session: &str,
    raw_target_url: &str,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let user_ns = "default";

    let Some(source_url) = state
        .book_service
        .source_for_login_session(login_session)
        .await
    else {
        return error_response(403, "invalid or expired loginSession");
    };
    let source = match state
        .book_source_service
        .get(&user_ns, &source_url)
        .await
    {
        Ok(Some(source)) => source,
        _ => return error_response(404, "bookSource not found"),
    };

    let target_url = match resolve_proxy_target_url(raw_target_url, &source.book_source_url) {
        Ok(url) => url,
        Err(err) => return error_response(400, &err.to_string()),
    };
    if !proxy_target_matches_source(&target_url, &source.book_source_url) {
        return error_response(403, "proxy target is outside the book source domain");
    }
    let upstream_referer = extract_upstream_referer(request.headers());
    match forward_book_source_request(
        &state.book_service,
        &source,
        login_session,
        request.method(),
        request.headers(),
        &target_url,
        upstream_referer.as_deref(),
        request.body(),
    )
    .await
    {
        Ok(response) => response,
        Err(err) => error_response(502, &err.to_string()),
    }
}
