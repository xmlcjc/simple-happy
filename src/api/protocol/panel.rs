use super::*;

/// `${readerOrigin}/sourcePanel?__readerPanelId=...`: 面板 HTML(已注入宿主桥)。
///
/// 面板由后端托管而不是用 iframe `srcdoc`, 因为 `srcdoc` 是 `about:srcdoc` 源,
/// 无法同源访问 `/sourcePanelBridge`; 挂在 `reader.localhost` 下才能同步回调宿主。
pub(super) async fn source_panel_route(
    state: &AppState,
    query: &str,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let params = parse_query(query);
    // Keep transport fields separate from the upstream page's own query.
    let namespaced = params.contains_key("__readerPanelId");
    let Some(panel_id) = params.get(if namespaced { "__readerPanelId" } else { "panelId" }) else {
        return text_response(400, "missing panelId");
    };
    let Some(entry) = crate::service::book_service::get_source_panel(panel_id) else {
        return text_response(404, "panel not found");
    };
    let target = params.get(if namespaced { "__readerPanelUrl" } else { "url" });
    // 面板页 JS 改写自身 URL 的 query 后整页导航时(某聚合源的登录页的
    // `sw('register')`: `new URL(location.href)` + searchParams.set +
    // location.href), 改动只落在代理 URL 的外层 query 上 —— 桥脚本的
    // proxyUrl() 把上游 query 复制到外层, 且初始加载会按设计删掉
    // __readerPanelUrl。只认 __readerPanelUrl 会让这类导航退回改写前的 URL,
    // 页面永远停在原状态(点注册始终停在登录页)。
    //
    // 注意这里**不改变**抓取/静态渲染的判据: 存有面板 HTML 的形式
    // (showBrowser(url, html), 如内嵌设置页)仍只在显式导航(target 存在)时
    // 抓取上游, 其余继续静态渲染, 保持既有行为。合并只影响进入 fetch 分支后
    // 实际请求的上游地址。
    let base = target.map(String::as_str).unwrap_or(&entry.url);
    let merged = merge_panel_outer_query(base, &params, namespaced);
    let navigation = merged.as_deref().or(target.map(String::as_str));
    if target.is_some() || (entry.html.is_empty() && !entry.url.starts_with("data:")) {
        return match fetch_source_panel_page(state, panel_id, &entry, navigation, request).await {
            Ok(response) => response,
            Err(error) => text_response(502, &error.to_string()),
        };
    }
    let html = render_source_panel_page(panel_id, &entry, &entry.url, &entry.html);
    http::Response::builder()
        .status(200)
        .header(http::header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(http::header::CACHE_CONTROL, "no-store")
        .body(html.into_bytes())
        .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

/// 把代理 URL 外层 query 合并进上游 URL。
///
/// 面板内 JS 用 `new URL(location.href)` + `searchParams.set` + `location.href`
/// 这类整页导航改写自身 query 时(某聚合源的登录页的 `sw('register')`), 改动只会
/// 落在代理 URL 的外层 query 上, 而 `__readerPanelUrl` 仍是改写前的地址。不透传
/// 就会永远停在原状态(点注册始终回到登录页)。
///
/// 合并采用「外层覆盖内层」语义: 同为 `mode` 时以外层为准, 其余上游参数保留。
/// 返回 `None` 表示无需合并(没有额外参数、外层与 base 一致、非 http(s)、或 base
/// 不是绝对 URL), 调用方沿用原 target。
pub(super) fn merge_panel_outer_query(
    base: &str,
    params: &HashMap<String, String>,
    namespaced: bool,
) -> Option<String> {
    let mut url = Url::parse(base).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    // 只排除本协议的传输字段。namespaced 形式下 `url`/`panelId` 可能是上游页面
    // 自己的业务参数, 不能一并丢掉。
    let transport: [&str; 2] = if namespaced {
        ["__readerPanelId", "__readerPanelUrl"]
    } else {
        ["panelId", "url"]
    };
    let mut outer: Vec<(&str, &str)> = params
        .iter()
        .filter(|(key, _)| !transport.contains(&key.as_str()))
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    // `params` 是 HashMap, 迭代序随机; 排序让追加顺序可复现(日志、缓存键)。
    outer.sort_unstable();
    if outer.is_empty() {
        return None;
    }

    let existing: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    // 面板内导航的常态是外层 query 复制自上游(`proxyUrl` 的 `proxy.search = url.search`),
    // 此时每个外层参数都已在 base 中原样存在, 无事可做 —— 返回 `None` 让调用方
    // 沿用原 target, 避免 query 解码/重编码往返(如 `%20` → `+`、`%zz` → `%25zz`)
    // 改写上游 URL 形式。上游按 query 原文签名/比对时, 这种改写会让请求失配。
    let pending: Vec<(&str, &str)> = outer
        .into_iter()
        .filter(|&(key, value)| {
            !existing
                .iter()
                .any(|(name, current)| name.as_str() == key && current.as_str() == value)
        })
        .collect();
    if pending.is_empty() {
        return None;
    }

    // 纯追加(无同名冲突, 即该页点注册这类场景)时直接在原 query 后拼接,
    // 保留既有片段原文 —— `query_pairs` 重建会把 `%20` 变 `+`、`%zz` 变
    // `%25zz`, 上游按 query 原文签名/比对时会失配。
    let overrides = pending
        .iter()
        .any(|(key, _)| existing.iter().any(|(name, _)| name.as_str() == *key));
    if !overrides {
        let mut query = url.query().unwrap_or_default().to_string();
        for (key, value) in &pending {
            if !query.is_empty() {
                query.push('&');
            }
            query.push_str(&urlencoding::encode(key));
            query.push('=');
            query.push_str(&urlencoding::encode(value));
        }
        url.set_query(Some(&query));
        return Some(url.to_string());
    }

    // 与桥脚本的 `searchParams.set` 一致: 同名参数先整体删除再追加, 而不是只覆盖
    // 第一项 —— 上游 `?a=1&a=2` 遇外层 `a=3` 应得 `?a=3`, 不是 `?a=3&a=2`。
    let mut merged: Vec<(String, String)> = existing
        .into_iter()
        .filter(|(name, _)| !pending.iter().any(|(key, _)| name == key))
        .collect();
    for (key, value) in pending {
        merged.push((key.to_string(), value.to_string()));
    }
    url.set_query(None);
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in &merged {
            pairs.append_pair(key, value);
        }
    }
    Some(url.to_string())
}

/// 组装面板页: 面板 HTML + 注入的宿主桥脚本。面板不存在时返回 `None`。
pub fn build_source_panel_page(panel_id: &str) -> Option<String> {
    let entry = crate::service::book_service::get_source_panel(panel_id)?;
    Some(render_source_panel_page(panel_id, &entry, &entry.url, &entry.html))
}

pub(super) fn script_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default().replace('<', "\\u003c")
}

pub(super) fn render_source_panel_page(panel_id: &str, entry: &crate::service::book_service::SourcePanelEntry, page_url: &str, html: &str) -> String {
    let jar_cookie = entry.http.as_ref()
        .and_then(|http| url::Url::parse(page_url).ok().map(|url| http.session.cookies.get(&url)))
        .unwrap_or_default();
    render_source_panel_page_with_cookie(panel_id, entry, page_url, html, &jar_cookie)
}

/// `jar_cookie`: 书源 cookie jar 中该页面域的 cookie 串 (`k=v; k2=v2`)。
/// 面板页面 JS 常用 `document.cookie` 判断登录态（如某书源 `/user` 的
/// `getCookie('qttoken')`）。自定义协议 origin 下 `Set-Cookie` 响应头
/// 未必被 WebView 落盘，这里直接在页面脚本之前注入并劫持
/// `document.cookie` 的 getter（jar 优先合并），保证入口检查可读到。
pub(super) fn render_source_panel_page_with_cookie(panel_id: &str, entry: &crate::service::book_service::SourcePanelEntry, page_url: &str, html: &str, jar_cookie: &str) -> String {
    let page_url = if page_url.starts_with("data:") { "" } else { page_url };
    let mut bridge = format!("<script>window.__readerPanelUrl={};window.__readerSourceKey={};</script>", script_string(page_url), script_string(&entry.book_source_url));
    let panel_url = script_string(page_url);
    // 即使 jar 里还没有 cookie 也要注入: 劫持的 setter 负责把页面写入的
    // document.cookie **回写书源 jar**。登录脚本用 `document.cookie = 'user_token=…'`
    // 记录会话, 而上游后续页面(/user_center 等)由**服务端**读 Cookie 判断登录态 ——
    // 只写浏览器的话这份 cookie 留在面板 origin(reader.localhost)下, 转发请求带不上,
    // 上游就会当作未登录并把页面踢回登录页(表现为「登录过期」)。
    if page_url.starts_with("http://") || page_url.starts_with("https://") {
        bridge.push_str(&format!(
            r#"<script>
(function () {{
  var jarCookie = {};
  var pageUrl = {};
  var pairs = jarCookie.split(/;\s*/).filter(function (p) {{ return p.indexOf('=') > 0; }});
  var jar = {{}};
  pairs.forEach(function (p) {{
    var i = p.indexOf('=');
    jar[p.slice(0, i).trim()] = p.slice(i + 1).trim();
  }});
  var desc = Object.getOwnPropertyDescriptor(Document.prototype, 'cookie')
    || Object.getOwnPropertyDescriptor(HTMLDocument.prototype, 'cookie')
    || Object.getOwnPropertyDescriptor(document, 'cookie');
  if (!desc || !desc.get || !desc.set) return;
  // 先尽力把 jar cookie 写进浏览器(部分环境可写, 子资源请求也能带上)
  pairs.forEach(function (p) {{
    try {{ desc.set.call(document, p.trim() + '; path=/'); }} catch (e) {{}}
  }});
  var nativeGet = desc.get, nativeSet = desc.set;
  // 页面写入的 cookie 回写书源 jar: 上游靠服务端读 Cookie 校验登录态, 只有落到
  // jar 里才能随转发请求送达。异步桥不可用(setter 是同步属性), 因此等宿主桥
  // 注入后用其中的同步桥提交; 桥尚未就绪时排队, 就绪后补发。
  var pending = [];
  function flush() {{
    if (!window.__readerPanelBridgeReady || !pending.length) return;
    var queued = pending; pending = [];
    queued.forEach(function (pair) {{
      try {{ window.__readerPanelCookieSet(pageUrl, pair); }} catch (e) {{}}
    }});
  }}
  window.__readerPanelQueueCookie = function (pair) {{ pending.push(pair); flush(); }};
  window.__readerPanelCookieFlush = flush;
  Object.defineProperty(document, 'cookie', {{
    configurable: true,
    get: function () {{
      var merged = {{}};
      try {{
        String(nativeGet.call(document)).split(/;\s*/).forEach(function (p) {{
          var i = p.indexOf('=');
          if (i > 0) merged[p.slice(0, i).trim()] = p.slice(i + 1).trim();
        }});
      }} catch (e) {{}}
      // 书源 jar 优先: 浏览器里的同名旧值被实时 jar 值覆盖
      Object.keys(jar).forEach(function (k) {{ merged[k] = jar[k]; }});
      return Object.keys(merged).map(function (k) {{ return k + '=' + merged[k]; }}).join('; ');
    }},
    set: function (value) {{
      try {{ nativeSet.call(document, value); }} catch (e) {{}}
      // 只取 `名字=值` 部分, 属性(expires/path/domain)交给 jar 侧按域处理。
      try {{
        var raw = String(value);
        var head = raw.split(';')[0].trim();
        if (head && head.indexOf('=') > 0) {{
          var name = head.slice(0, head.indexOf('=')).trim();
          var val = head.slice(head.indexOf('=') + 1).trim();
          // 页面用 `x=; expires=<过去时间>` 删除 cookie(该页的 userLogout 即如此)。
          // 空值/已过期应把条目从 jar 移除, 否则空值会被当成有效登录态回写上游。
          // 注意只取 `expires=` 之后到下一个 `;` 之前的部分交给 Date.parse ——
          // 整串(含其它属性)会解析成 NaN, 使过期判断永远不成立。
          var expired = false;
          var expires = /;\s*expires\s*=\s*([^;]+)/i.exec(raw);
          if (expires) {{
            var at = Date.parse(expires[1].trim());
            expired = !isNaN(at) && at <= Date.now();
          }}
          if (expired || !val) {{
            delete jar[name];
            // 删除必须发**空值**(`name=`), 而不是原样发 `head`: 宿主侧按「值为空」
            // 判定删除, 发非空值会被当成有效 cookie 写回 jar, 结果删不掉。
            if (window.__readerPanelQueueCookie) window.__readerPanelQueueCookie(name + '=');
            return;
          }}
          jar[name] = val;
          if (window.__readerPanelQueueCookie) window.__readerPanelQueueCookie(head);
        }}
      }} catch (e) {{}}
    }}
  }});
}})();
</script>"#,
            script_string(jar_cookie),
            panel_url
        ));
    }
    bridge.push_str(&build_source_panel_bridge_script(panel_id, entry.external_links));
    if let Some(preload) = &entry.preload_js {
        bridge.push_str(&format!("<script>(0,eval)({});</script>", script_string(preload)));
    }
    if page_url.starts_with("http://") || page_url.starts_with("https://") {
        let base = page_url.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
        bridge.push_str(&format!("<base href=\"{base}\">"));
    }
    // Install the bridge and preload before scripts in the original head execute.
    let head = Regex::new(r"(?i)<head\b[^>]*>").expect("head pattern");
    if let Some(found) = head.find(html) {
        format!("{}{}{}", &html[..found.end()], bridge, &html[found.end()..])
    } else {
        format!("{bridge}{html}")
    }
}

pub(super) async fn fetch_source_panel_page(
    state: &AppState, panel_id: &str, entry: &crate::service::book_service::SourcePanelEntry,
    target: Option<&str>, request: &tauri::http::Request<Vec<u8>>,
) -> Result<tauri::http::Response<Vec<u8>>, AppError> {
    let target = resolve_proxy_target_url(target.unwrap_or(&entry.url), &entry.url)?;

    // RSS / 单 URL 订阅源的面板没有归属书源(`book_source_url` 为空), 因此不能走
    // 下面那条「查书源 → 用书源规则」的路径。它对应手机端 `ReadRssActivity` 的
    // 那个 WebView: `handleCommonSchemes` 对 http/https 返回 false, 也就是
    // 「在原地打开」, 不限制域名。源仓库的二级页面正是跨域跳转
    // (yckceo.vip → yckceo.com / yck2026.fun), 若沿用书源的同域校验只会得到
    // 「panel target is outside the page domain」。
    if entry.book_source_url.is_empty() {
        return fetch_rss_panel_page(panel_id, entry, &target, request).await;
    }

    // 书源面板保持严格同域: 它带着书源的 cookie 与登录态, 放宽等于把登录凭证
    // 交给任意目标域。
    if !proxy_target_matches_source(&target, &entry.url) {
        return Ok(text_response(403, "panel target is outside the page domain"));
    }
    let source = state.book_source_service.get("default", &entry.book_source_url).await?
        .ok_or_else(|| AppError::NotFound("bookSource not found".into()))?;
    fetch_source_panel_response(&source, panel_id, entry, &target, request).await
}

/// RSS 面板的二级页抓取: 不依赖书源, 用面板自身的 http 上下文(源级 cookie jar
/// 与代理), 并保留内网地址拦截 —— 面板 HTML 托管在 `reader` origin, 桥能触达
/// 宿主网络, 而 RSS 源可从远程 JSON 导入(不受信任)。
pub(super) async fn fetch_rss_panel_page(
    panel_id: &str,
    entry: &crate::service::book_service::SourcePanelEntry,
    target: &str,
    request: &tauri::http::Request<Vec<u8>>,
) -> Result<tauri::http::Response<Vec<u8>>, AppError> {
    crate::api::commands::rss::reject_non_public_target(target)?;
    let context = match entry.http.clone() {
        Some(context) => context,
        None => crate::parser::js::source_http_context(&entry.url)
            .map_err(AppError::Internal)?,
    };
    let client = context.client.clone();
    let method = request.method().clone();
    let mut forwarded = http::HeaderMap::new();
    if let Some(content_type) = request.headers().get(http::header::CONTENT_TYPE) {
        forwarded.insert(http::header::CONTENT_TYPE, content_type.clone());
    }
    let mut builder = client.request(method, target).headers(forwarded);
    if !request.body().is_empty() {
        builder = builder.body(request.body().clone());
    }
    let response = builder.send().await.map_err(AppError::Http)?;
    let status = response.status();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = response.bytes().await.map_err(AppError::Http)?;
    let is_html = is_html_response(Some(&content_type), &bytes);
    let body = if is_html {
        let html = crate::crawler::fetcher::decode_body(&bytes, None, Some(&content_type));
        // 跨域放行: 这类页面会跳到任意同族/合作域名。
        let html = rewrite_source_panel_html(&html, &final_url, &entry.url, panel_id);
        render_source_panel_page(panel_id, entry, &final_url, &html).into_bytes()
    } else {
        bytes.to_vec()
    };
    let built = http::Response::builder()
        .status(status.as_u16())
        .header(
            http::header::CONTENT_TYPE,
            if is_html { "text/html; charset=utf-8" } else { &content_type },
        )
        .header(http::header::CACHE_CONTROL, "no-store");
    Ok(built.body(body).unwrap_or_default())
}

pub(super) async fn fetch_source_panel_response(
    source: &BookSource, panel_id: &str, entry: &crate::service::book_service::SourcePanelEntry,
    target: &str, request: &tauri::http::Request<Vec<u8>>,
) -> Result<tauri::http::Response<Vec<u8>>, AppError> {
    let context = entry.http.clone().map(Ok).unwrap_or_else(|| crate::parser::js::source_http_context(&entry.book_source_url))
        .map_err(AppError::Internal)?;
    let header_source = source.clone();
    let header_context = context.clone();
    let headers = tokio::task::spawn_blocking(move || {
        crate::parser::js::with_http_context(header_context, || {
            crate::parser::js::with_source(&header_source, || {
                crate::crawler::url_analyzer::source_header_spec(&header_source)
            })
        })
    }).await.map_err(|err| AppError::Internal(err.into()))??;
    let client = match headers.proxy.as_deref() {
        Some(proxy) => crate::crawler::http_client::build_source_client(context.timeout_secs, Some(proxy), context.session.cookies.clone()).map_err(AppError::Internal)?,
        None => context.client.clone(),
    };
    let mut forwarded = merge_proxy_headers(headers.headers, request.headers())?;
    if !forwarded.contains_key(http::header::REFERER) {
        forwarded.insert(http::header::REFERER, http::HeaderValue::from_str(&entry.url)
            .map_err(|err| AppError::BadRequest(format!("网页来源地址无效: {err}")))?);
    }
    // WebView2 交给宿主的自定义协议请求可能不带 Content-Type, 也可能带一个与实际
    // 请求体不符的头。后端(PHP 等)在缺该头、或头与体不匹配时都**完全不解析请求体**
    // (`$_POST` 为空), 表现为「后端收不到任何字段」—— 某聚合源的注册接口即回
    // 「无效的操作」。因此以请求体的真实字节为准: 体是 multipart 就按体首行的
    // boundary 声明, 否则仅在缺头时按 urlencoded 兜底。
    if request.method() == http::Method::POST {
        inherit_form_content_type(&mut forwarded, request.body());
    }
    let mut builder = client.request(request.method().clone(), target).headers(forwarded);
    if !request.body().is_empty() { builder = builder.body(request.body().clone()); }
    let response = builder.send().await.map_err(AppError::Http)?;
    let status = response.status();
    let final_url = response.url().to_string();
    let content_type = response.headers().get(http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_string();
    let bytes = response.bytes().await.map_err(AppError::Http)?;
    let is_html = is_html_response(Some(&content_type), &bytes);
    let body = if is_html {
        let html = crate::crawler::fetcher::decode_body(&bytes, None, Some(&content_type));
        let html = rewrite_source_panel_html(&html, &final_url, &entry.url, panel_id);
        let jar_cookie = url::Url::parse(&final_url)
            .map(|url| context.session.cookies.get(&url)).unwrap_or_default();
        render_source_panel_page_with_cookie(panel_id, entry, &final_url, &html, &jar_cookie).into_bytes()
    } else { bytes.to_vec() };
    let mut built = http::Response::builder().status(status.as_u16())
        .header(http::header::CONTENT_TYPE, if is_html { "text/html; charset=utf-8" } else { &content_type })
        .header(http::header::CACHE_CONTROL, "no-store");
    // 面板页在 reader origin 里运行, 页面 JS 读 document.cookie 判断登录态
    // (如面板页的 getCookie('qttoken'))。真实 cookie 在书源 jar 中,
    // 以 Set-Cookie 透出给面板 origin, 让入口检查通过; 后续请求仍由代理
    // 携带 jar cookie, 浏览器侧的这份仅供 JS 读取。每个 cookie 对一个头。
    if is_html {
        if let Ok(url) = url::Url::parse(&final_url) {
            for pair in context.session.cookies.get(&url).split("; ").filter(|p| !p.is_empty()) {
                built = built.header(http::header::SET_COOKIE, format!("{pair}; Path=/"));
            }
        }
    }
    Ok(built.body(body).unwrap_or_default())
}

/// 把上游 HTML 里的资源/链接改写成经本应用代理的地址。
///
/// 只改写**同域**目标。跨域资源(第三方 CDN 的 css/js/图片、广告统计等)保持原样,
/// 由 WebView 直接去取 —— 这是刻意的:
///
///   1. 每个改写过的地址都会变成一次 `/sourcePanel` 宿主请求, 而宿主请求是
///      **串行经过本进程**的(reqwest → 上游 → 回写)。页面引用几十个跨域资源时,
///      就是几十个并发任务, 每个都要占住连接的 15s 超时才释放; WebView2 对同一
///      host 的并发连接数又有上限(通常 6), 于是请求互相排队直到超时。
///   2. 跨域页面往往引用大量第三方资源, 其中不少在直连网络下根本连不通。
///      一旦把它们全部收进代理, 主文档加载会被这些挂起的子资源拖住。
///
/// 实测: 放开跨域改写后打开源仓库二级页, 系统出现大量挂起连接, 资源管理器
/// (explorer.exe) 被拖到重启。因此这里保持「只代理同域」的保守做法; 跨域二级页
/// 本身仍由 `fetch_rss_panel_page` 直接抓取并托管(不受同域限制), 只是它引用的
/// 外域资源不再经过我们中转。
pub(super) fn rewrite_source_panel_html(
    html: &str,
    page_url: &str,
    allowed_url: &str,
    panel_id: &str,
) -> String {
    let tags = Regex::new(r"(?is)<(?:script|link|img|iframe|source|video|audio|form|a)\b[^>]*>").unwrap();
    let attrs = Regex::new(r#"(?i)\b(src|href|action)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap();
    tags.replace_all(html, |tag: &Captures| {
        attrs.replace_all(&tag[0], |attr: &Captures| {
            let raw = attr.get(2).or_else(|| attr.get(3)).map(|v| v.as_str()).unwrap_or_default();
            if raw.starts_with('#') || raw.is_empty() { return attr[0].to_string(); }
            let target = crate::crawler::url_analyzer::absolute_url(page_url, &raw.replace("&amp;", "&"));
            // 跨域面板也按「本次页面所在域」判定, 而不是按订阅源初始地址 ——
            // 否则跨域二级页连自己的资源都会被判成外站。
            if !proxy_target_matches_source(&target, page_url)
                && !proxy_target_matches_source(&target, allowed_url)
            {
                return attr[0].to_string();
            }
            format!("{}=\"{}/sourcePanel?__readerPanelId={}&amp;__readerPanelUrl={}\"", &attr[1], reader_scheme_origin(), urlencoding::encode(panel_id), urlencoding::encode(&target))
        }).into_owned()
    }).into_owned()
}

/// 面板内桥的同步端点。请求体: `{ "action": "...", "payload": {...} }`
/// 响应体: `{ "ok": true, "value": ... }` 或 `{ "ok": false, "error": "..." }`
pub(super) async fn source_panel_bridge_route(
    state: &AppState,
    query: &str,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    // 信任边界: 面板 HTML 来自用户导入的书源, 而桥能触达宿主的网络与脚本能力。
    // 只有本应用自己的 origin 能调用, 外部网页(带具体 Origin)一律拒绝。
    let request_origin = request
        .headers()
        .get(http::header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if !panel_bridge_origin_allowed(request_origin) {
        return json_response(403, serde_json::json!({ "ok": false, "error": "forbidden origin" }));
    }
    // 无 Origin 头是同源请求(面板与桥同为 `reader` 协议 origin), 不需要 CORS 头;
    // 带 Origin 且已通过上面的校验时回显它, 绝不使用通配符。
    let allow_origin = request_origin.map(|_| reader_scheme_origin());

    let params = parse_query(query);
    let Some(panel_id) = params.get("panelId") else {
        return json_response_panel(
            400,
            serde_json::json!({ "ok": false, "error": "missing panelId" }),
            allow_origin,
        );
    };
    let Some(entry) = crate::service::book_service::get_source_panel(panel_id) else {
        return json_response_panel(
            404,
            serde_json::json!({ "ok": false, "error": "panel not found" }),
            allow_origin,
        );
    };
    let body: serde_json::Value = serde_json::from_slice(request.body()).unwrap_or_default();
    let action = body.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let payload = body.get("payload").cloned().unwrap_or_default();

    if action == "complete" {
        let page = crate::service::source_browser::BrowserPage {
            url: payload.get("url").and_then(|v| v.as_str()).filter(|v| !v.is_empty()).unwrap_or(&entry.url).to_string(),
            body: payload.get("body").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        };
        let result = match crate::service::source_browser::complete_browser(panel_id, page) {
            Ok(()) => serde_json::json!({"ok":true}),
            Err(error) => serde_json::json!({"ok":false,"error":error}),
        };
        return json_response_panel(200, result, allow_origin);
    }

    let outcome = crate::service::book_service::handle_source_panel_bridge(
        &state.book_source_service,
        &entry,
        action,
        payload,
    )
    .await;
    match outcome {
        Ok(value) => json_response_panel(
            200,
            serde_json::json!({ "ok": true, "value": value }),
            allow_origin,
        ),
        Err(error) => json_response_panel(
            200,
            serde_json::json!({ "ok": false, "error": error }),
            allow_origin,
        ),
    }
}

/// 面板桥的调用者是否可信。
///
/// 放行两种情况: 不带 `Origin`(同源同步 XHR / 原生调用), 或 Origin 等于
/// `reader` 协议 origin(面板自身)。其余(如外部 https 页面)一律拒绝 ——
/// 否则任何本机网页都能借桥驱动宿主去请求书源接口或执行脚本。
pub(super) fn panel_bridge_origin_allowed(origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(value) => value.trim() == reader_scheme_origin(),
    }
}

pub(super) fn parse_query(query: &str) -> HashMap<String, String> {
    url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

pub(super) fn text_response(status: u16, message: &str) -> tauri::http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

pub(super) fn json_response(status: u16, value: serde_json::Value) -> tauri::http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(http::header::CACHE_CONTROL, "no-store")
        .body(value.to_string().into_bytes())
        .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

/// 面板桥专用响应。
///
/// 面板与桥同为 `reader` 协议 origin(因此请求通常不带 `Origin`)。只有调用方
/// 确实带了 `Origin` 时才回显**该已通过校验的 origin**, 不再使用通配符 ——
/// 通配符会让任意本机页面都能读走桥的响应。
pub(super) fn json_response_panel(
    status: u16,
    value: serde_json::Value,
    allow_origin: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    let mut builder = http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(http::header::CACHE_CONTROL, "no-store")
        .header(http::header::VARY, "Origin");
    if let Some(origin) = allow_origin {
        builder = builder.header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    builder
        .body(value.to_string().into_bytes())
        .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

// ─────────────────────────── cover / epub / files ───────────────────────────
