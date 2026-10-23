//! Custom `reader` URI-scheme handler.
//!
//! Serves cover images, EPUB assets, uploaded files and the bookSourceProxy
//! login iframe over the reader scheme so `<img>`/`<iframe>` in the frontend
//! keep loading synchronously without async blob-URL plumbing.
//!
//! The scheme origin is platform-dependent: WebView2 (Windows) serves custom
//! schemes as `http://reader.localhost`, WebKitGTK (Linux) and WKWebView
//! (macOS) as `reader://localhost`. See [`reader_scheme_origin`].

use crate::api::AppState;
use crate::error::error::{ApiResponse, AppError};
use crate::model::book_source::BookSource;
use crate::util::text::{normalize_source_url, repair_encoded_url};
use serde::Deserialize;
use regex::{Captures, Regex};
use std::collections::HashMap;
use tauri::http;
use tauri::Manager;
use url::Url;

/// Origin of the custom `reader` URI scheme, which differs per webview
/// backend: WebView2 (Windows/Android) serves custom schemes as
/// `http://<scheme>.localhost`, WebKitGTK/WKWebView (Linux/macOS) as
/// `<scheme>://localhost`. Mirrors the mapping in Tauri's injected
/// `convertFileSrc` used by the frontend (`frontend/src/api/scheme.ts`).
pub fn reader_scheme_origin() -> &'static str {
    if cfg!(any(target_os = "windows", target_os = "android")) {
        "http://reader.localhost"
    } else {
        "reader://localhost"
    }
}

/// Entry point registered via `register_asynchronous_uri_scheme_protocol("reader", ...)`.
pub fn handle_reader_scheme(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    let path = request.uri().path().to_string();
    let query = request.uri().query().unwrap_or_default().to_string();
    let app_handle = ctx.app_handle().clone();

    tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<AppState>();
        let response = route(&state, &path, &query, request).await;
        responder.respond(response);
    });
}

/// 协议层的路由分类。
///
/// 单独抽出来是为了能在不构造 `AppState` 的前提下断言"每个已知路径都注册了
/// 处理器" —— 拆分本文件时漏掉一条路由，测试会立刻发现，而不是等用户点开
/// 封面/PDF 时才发现 404。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteKind {
    BookSourceProxy,
    SourcePanelBridge,
    SourcePanel,
    Cover,
    Epub,
    Pdf,
    Files,
    MusicStream,
    MusicLocal,
    LoginProxy,
    Fallback,
}

fn classify_route(path: &str) -> RouteKind {
    match path {
        "/bookSourceProxy" => RouteKind::BookSourceProxy,
        "/sourcePanelBridge" => RouteKind::SourcePanelBridge,
        "/sourcePanel" => RouteKind::SourcePanel,
        "/cover" => RouteKind::Cover,
        "/epub" => RouteKind::Epub,
        "/pdf" => RouteKind::Pdf,
        "/files" => RouteKind::Files,
        "/musicStream" => RouteKind::MusicStream,
        "/musicLocal" => RouteKind::MusicLocal,
        // 登录页兜底: 登录 iframe 跑在专属路径空间里, 并**完整保留上游路径**:
        //   http://reader.localhost/__login/<session>/user/me?app=yuedu
        //
        // 这两个性质缺一不可:
        // 1. 打包产出的单页应用把路径写死进代码 —— Vue Router 的 base 是
        //    `/user/`, 分片也可能是 `"/user/" + chunk`。文档路径一旦对不上,
        //    这些绝对路径全部 404, 应用永远挂载不起来(纯白页)。
        // 2. WebView2 在自定义协议下**不给子资源发 Cookie, Referer 也是空串**
        //    (实测), 因此会话只能编码进路径, 让每个分片请求天然带着它。
        p if p.starts_with(SESSION_PATH_PREFIX) => RouteKind::LoginProxy,
        _ => RouteKind::Fallback,
    }
}

async fn route(
    state: &AppState,
    path: &str,
    query: &str,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    match classify_route(path) {
        RouteKind::BookSourceProxy => book_source_proxy_route(state, query, &request).await,
        RouteKind::SourcePanelBridge => source_panel_bridge_route(state, query, &request).await,
        RouteKind::SourcePanel => source_panel_route(state, query, &request).await,
        RouteKind::Cover => cover_route(state, query).await,
        RouteKind::Epub => epub_route(state, query).await,
        RouteKind::Pdf => pdf_route(state, query).await,
        RouteKind::Files => files_route(state, query).await,
        // 音乐取流: 插件播放地址常要求 Referer/UA, 由 Rust 侧带上头部取回字节,
        // 前端再转 Blob 播放。具体逻辑留在 service::music, 这里只做协议适配。
        RouteKind::MusicStream => music_stream_route(state, query).await,
        // 本地音乐: 只放行曲库里登记过的路径, 避免任意文件读取。
        RouteKind::MusicLocal => music_local_route(state, query).await,
        RouteKind::LoginProxy => source_login_proxy_route(state, path, query, &request).await,
        RouteKind::Fallback => source_login_fallback_route(state, path, query, &request)
            .await
            .unwrap_or_else(|| {
                http::Response::builder()
                    .status(404)
                    .body(Vec::new())
                    .unwrap_or_default()
            }),
    }
}

/// 登录页专属路径空间的前缀: `/__login/<session>/<上游路径>`。
const SESSION_PATH_PREFIX: &str = "/__login/";

mod assets;
mod login;
mod music;
mod panel;
mod proxy;
mod scripts;
mod source_proxy;

// 子模块之间互相引用（以及测试里的裸名）都通过这一层扁平命名空间解析；
// 搬动函数只改文件归属，不改调用写法。
use assets::*;
use login::*;
use music::*;
use panel::*;
use proxy::*;
use scripts::*;
use source_proxy::*;

// 对外路径保持 `reader_rust::api::protocol::build_source_panel_page` 不变。
pub use panel::build_source_panel_page;

// 这三个 `pub(crate)` 函数被协议层以外的模块按全路径调用，必须从本模块再导出，
// 否则经由私有 glob 绑定时路径不可见。
pub(crate) use assets::remove_cover_cache_url;
pub(crate) use proxy::{proxy_target_matches_source, resolve_panel_cookie_url};

#[cfg(test)]
mod tests;
