use super::*;

/// 书源面板(`java.showBrowser`)注入的宿主桥。
///
/// 插件面板通过 `window.java` / `window.cache` / `window.run` 回调宿主
/// (legado `WebJsExtensions`)。面板里 `java.ajax` 是**同步**调用, 而 Tauri IPC
/// 是异步的, 因此这里把 bridge 请求发到同源的 `/sourcePanelBridge`, 用同步
/// `XMLHttpRequest` 取回结果 —— 与 legado 的同步 Java 桥语义一致。
///
/// `external_links`: 面板内的二级链接是否交给系统浏览器打开(见
/// `SourcePanelEntry::external_links`)。
///
/// 注意: 二级链接现在**统一**留在面板内经代理导航(见 click 兜底那段注释),
/// 这个标志暂时没有消费方, 保留是为了让 `SourcePanelEntry` 的既有语义与
/// 备份/调用方不受影响, 也便于将来按需恢复「RSS 外链走系统浏览器」。
pub(super) fn build_source_panel_bridge_script(panel_id: &str, external_links: bool) -> String {
    let panel_id_json = script_string(panel_id);
    let external_links_json = if external_links { "true" } else { "false" };
    format!(
        r#"<script>
(function () {{
  var panelId = {panel_id_json};
  // 见 Rust 侧 build_source_panel_bridge_script 的说明: 当前仅为兼容保留。
  var externalLinks = {external_links_json};
  var bridgeBase = new URL("/sourcePanelBridge?panelId=" + encodeURIComponent(panelId), window.location.href).href;
  var nativeFetch = window.fetch.bind(window);
  var pageUrl = window.__readerPanelUrl || "";
  var documentId = window.crypto && window.crypto.randomUUID ? window.crypto.randomUUID() : String(Date.now()) + "-" + Math.random();

  // 同步桥: 与 legado 的 Java 同步接口语义一致(插件大量使用同步 ajax)。
  //
  // 面板 iframe 保留 `allow-same-origin`(插件用 localStorage 缓存书籍信息,
  // opaque origin 下会抛 SecurityError)。桥与面板同为 reader 协议 origin,
  // 因此这是同源请求; 服务端仍会校验 Origin, 只放行 reader origin 自身。
  function bridgeSync(action, payload) {{
    var xhr = new XMLHttpRequest();
    var body = JSON.stringify({{ action: action, payload: payload || {{}} }});
    xhr.open("POST", bridgeBase, false); // false = 同步
    xhr.setRequestHeader("Content-Type", "text/plain;charset=UTF-8");
    try {{
      xhr.send(body);
    }} catch (e) {{
      return {{ ok: false, error: String(e) }};
    }}
    if (xhr.status < 200 || xhr.status >= 300) {{
      return {{ ok: false, error: "bridge HTTP " + xhr.status }};
    }}
    try {{
      return JSON.parse(xhr.responseText);
    }} catch (e) {{
      return {{ ok: false, error: "bridge bad response" }};
    }}
  }}

  // 面板页 JS 用 `document.cookie = 'k=v'` 记录登录态(某聚合源的登录页写
  // `user_token`), 而上游后续页面由**服务端**读 Cookie 判断登录, 只留在浏览器里
  // 转发请求带不上, 上游会当作未登录并踢回登录页。这里把页面写入的 cookie 经
  // 同步桥落到书源 jar。注入顺序上 cookie 劫持脚本在本脚本之前, 它会把待发送的
  // 条目排队, 由这里的 ready 标志触发补发。
  window.__readerPanelCookieSet = function (url, pair) {{
    bridgeSync("cookieSet", {{ url: String(url || pageUrl), cookie: String(pair) }});
  }};
  window.__readerPanelBridgeReady = true;
  if (window.__readerPanelCookieFlush) {{ try {{ window.__readerPanelCookieFlush(); }} catch (e) {{}} }}

  // 通知宿主(父窗口): 面板要关闭 / 要换成另一个面板。
  // 面板 origin 为 reader.localhost, 父应用为 tauri.localhost, 用 postMessage 跨源通信。
  function postToHost(message) {{
    try {{
      message.panelId = panelId;
      message.documentId = documentId;
      window.parent.postMessage(message, "*");
    }} catch (e) {{}}
  }}


  var initialPageUrl = pageUrl;
  var panelOrigin = new URL(bridgeBase).origin;
  var navigating = false;
  var nativeSubmit = HTMLFormElement.prototype.submit;
  function panelTarget(url) {{
    return url.searchParams.get(url.searchParams.has("__readerPanelId") ? "__readerPanelUrl" : "url");
  }}
  function isPanelUrl(url) {{
    var id = url.searchParams.get(url.searchParams.has("__readerPanelId") ? "__readerPanelId" : "panelId");
    return url.origin === panelOrigin && url.pathname === "/sourcePanel" && id === panelId;
  }}
  // 面板页 JS 用 location.href 改写自身 query 做整页导航时(某聚合源的登录页的
  // sw('register')), 改动落在代理 URL 的外层 query 上, 而初始加载会按设计删掉
  // __readerPanelUrl。不并回真实 URL, 导航就会退回改动前的地址 —— 点注册永远
  // 停在登录页。同名参数以**外层为准**且整体替换(与 searchParams.set 一致),
  // 与服务端 merge_panel_outer_query 的语义严格对应。
  function mergeOuterQuery(from, to) {{
    var namespaced = from.searchParams.has("__readerPanelId");
    var skip = namespaced ? ["__readerPanelId", "__readerPanelUrl"] : ["panelId", "url"];
    var pending = [];
    from.searchParams.forEach(function (value, key) {{
      if (skip.indexOf(key) >= 0) return;
      // 已原样存在则跳过: 面板内导航的外层 query 常态是复制自上游, 此时不触碰
      // to 的 query, 避免 searchParams 重新序列化(%20 → +、%zz → %25zz)改写
      // 上游 URL 形式。
      if (to.searchParams.getAll(key).indexOf(value) >= 0) return;
      pending.push([key, value]);
    }});
    if (!pending.length) return to;
    // 无同名冲突时直接追加到原文末尾, 与 Rust 侧的纯追加路径一致, 不改写既有
    // query 片段; 有冲突才走 searchParams.delete + set(整体替换同名项)。
    var conflicts = pending.some(function (pair) {{ return to.searchParams.has(pair[0]); }});
    if (conflicts) {{
      for (var i = 0; i < pending.length; i++) {{
        to.searchParams.delete(pending[i][0]);
        to.searchParams.set(pending[i][0], pending[i][1]);
      }}
      return to;
    }}
    var extra = pending.map(function (pair) {{
      return encodeURIComponent(pair[0]) + "=" + encodeURIComponent(pair[1]);
    }}).join("&");
    var raw = to.search.replace(/^\?/, "");
    to.search = (raw ? raw + "&" : "") + extra;
    return to;
  }}
  function realUrl(raw) {{
    var url = new URL(raw, pageUrl || window.location.href);
    if (isPanelUrl(url)) {{
      var original = panelTarget(url) || initialPageUrl;
      if (original) {{
        var decoded = mergeOuterQuery(url, new URL(original));
        if (url.hash) decoded.hash = url.hash;
        return decoded;
      }}
    }}
    return url;
  }}
  function proxyUrl(url, storedHtml) {{
    var proxy = new URL("/sourcePanel", bridgeBase);
    proxy.search = url.search;
    proxy.searchParams.set("__readerPanelId", panelId);
    if (storedHtml) proxy.searchParams.delete("__readerPanelUrl");
    else proxy.searchParams.set("__readerPanelUrl", url.href);
    proxy.hash = url.hash;
    return proxy.href;
  }}
  // Pages read location.search during their first inline script (e.g. comment
  // book/chapter ids). Publish it before preload/page scripts, without fetching
  // again or turning a stored HTML panel into an upstream navigation on reload.
  if (/^https?:/.test(pageUrl)) {{
    var loadedUrl = new URL(window.location.href);
    window.history.replaceState(window.history.state, "", proxyUrl(new URL(pageUrl), !panelTarget(loadedUrl)));
  }}
  function updatePageUrl(url) {{
    pageUrl = url.href;
    window.__readerPanelUrl = pageUrl;
    var base = document.querySelector("base");
    if (base && /^https?:$/.test(url.protocol)) base.href = pageUrl;
    postToHost({{ type: "location", url: pageUrl }});
  }}
  function startNavigation(event) {{
    navigating = true;
    postToHost({{ type: "navigating" }});
    if (event && event.signal) event.signal.addEventListener("abort", function () {{
      navigating = false;
      postToHost({{ type: "navigationCancelled" }});
    }}, {{ once: true }});
  }}
  // 书源仓库的导入协议: legado://import/[path]?src=[json] 与
  // yuedu://booksource/importonline?src=[json](手机端 Legado 的
  // OnLineImportActivity / 源仓库页面用的都是这两个)。
  // 返回规范化后的 href, 非导入协议返回空串。
  function importProtocolUrl(raw) {{
    try {{
      var url = new URL(raw, pageUrl || window.location.href);
      if (!/^(legado|yuedu):$/i.test(url.protocol)) return "";
      return url.href;
    }} catch (error) {{
      return "";
    }}
  }}
  function requestImport(href) {{
    postToHost({{ type: "importProtocol", url: href }});
  }}
  function submitToProxy(event, url) {{
    var source = event.sourceElement;
    var original = source && (source.form || (source.tagName === "FORM" ? source : null));
    var form = document.createElement("form");
    form.method = "post";
    form.action = proxyUrl(url);
    form.enctype = source && source.getAttribute("formenctype") || original && original.enctype || "application/x-www-form-urlencoded";
    if (original) form.acceptCharset = original.acceptCharset;
    form.hidden = true;
    for (var pair of event.formData.entries()) {{
      var field = document.createElement("input");
      field.name = pair[0];
      if (typeof pair[1] === "string") {{
        field.type = "hidden";
        field.value = pair[1];
      }} else {{
        field.type = "file";
        var transfer = new DataTransfer();
        transfer.items.add(pair[1]);
        field.files = transfer.files;
      }}
      form.appendChild(field);
    }}
    (document.body || document.documentElement).appendChild(form);
    nativeSubmit.call(form);
    form.remove();
  }}
  if (window.navigation && typeof window.navigation.addEventListener === "function") {{
    window.navigation.addEventListener("navigate", function (event) {{
      if (event.downloadRequest || event.hashChange) return;
      var destination = new URL(event.destination.url);
      var source = event.sourceElement;
      var form = source && (source.form || (source.tagName === "FORM" ? source : null));
      var target = realUrl(destination.href);
      var rebuildGet = false;
      if (form && !event.formData) {{
        var action = new URL(source.getAttribute("formaction") || form.action || window.location.href, pageUrl || window.location.href);
        if (isPanelUrl(action)) {{
          // GET submissions replace the action query, including the proxy's id.
          target = realUrl(action.href);
          target.search = destination.search;
          target.hash = destination.hash;
          rebuildGet = true;
        }}
      }}
      if (event.destination.sameDocument && !rebuildGet) return;
      if (isPanelUrl(destination) && !rebuildGet) {{
        startNavigation(event);
        return;
      }}
      // 书源仓库的「一键导入」链接: legado://import/...?src= 与
      // yuedu://booksource/importonline?src=。这类协议系统浏览器不认识,
      // 交出去就是静默失败, 必须在面板里截下并转给宿主导入。
      var importTargetUrl = importProtocolUrl(destination.href) || importProtocolUrl(event.destination.url);
      if (importTargetUrl) {{
        event.preventDefault();
        requestImport(importTargetUrl);
        return;
      }}
      if (!/^https?:$/.test(target.protocol)) return;
      // 二级页面统一留在面板内。
      //
      // 早先这里把 RSS 面板的二级链接交给系统浏览器, 理由是上游的
      // X-Frame-Options / CSP frame-ancestors 会让 iframe 拒绝渲染(白屏)。
      // 但那个理由只在**直接内嵌上游地址**时成立: 现在跳转走 `proxyUrl`,
      // 由后端把页面抓回来重写后托管在 reader origin 上, 而代理只透传
      // Content-Type / Cache-Control / Set-Cookie, 不会转发这些限制头,
      // 所以经代理渲染不会白屏。改成留在面板内之后, 用户在源仓库里点进去
      // 的详情/列表页不再跳出应用。
      if (!event.cancelable) {{
        startNavigation(event);
        return;
      }}
      event.preventDefault();
      try {{
        if (!event.formData && pageUrl) {{
          var current = new URL(pageUrl);
          if (current.origin === target.origin && current.pathname === target.pathname && current.search === target.search && current.hash !== target.hash) {{
            updatePageUrl(target);
            window.location.hash = target.hash;
            return;
          }}
        }}
        if (event.formData) submitToProxy(event, target);
        else if (event.navigationType === "replace") window.location.replace(proxyUrl(target));
        else window.location.assign(proxyUrl(target));
      }} catch (error) {{
        navigating = false;
        postToHost({{ type: "navigationError", message: "网页跳转失败，请重新加载后重试：" + String(error) }});
      }}
    }});
  }}
  // Older webviews may not expose cancellable navigation. The host still
  // invalidates readiness when the frame leaves and verifies the next document.
  window.addEventListener("beforeunload", function () {{ if (!navigating) startNavigation(); }});

  // click 兜底: `window.navigation` 不可用时(老 WebView)上面的 navigate 拦截
  // 不会触发, 只能靠捕获阶段的 click 拦下 <a> 跳转。用事件委托而不是改写
  // href: 页面可能动态插入链接(JS 渲染的列表)。
  //
  // 两种面板合并成同一段逻辑(此前 RSS 与书源各写了一份, 行为还不一致):
  //   - `legado://` / `yuedu://` 导入协议 → 交给宿主导入;
  //   - http(s) 链接 → 一律改走代理, 留在面板内打开。
  //
  // RSS 面板早先把链接交给系统浏览器, 理由是上游 X-Frame-Options 会让 iframe
  // 白屏; 但那只是**直接内嵌上游地址**时的问题, 走 `proxyUrl` 后页面由后端
  // 抓取重写再托管, 代理不透传这些限制头, 因此不会白屏。
  document.addEventListener("click", function (event) {{
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    var anchor = event.target && event.target.closest ? event.target.closest("a[href]") : null;
    if (!anchor || anchor.hasAttribute("download")) return;
    var raw = anchor.getAttribute("href") || "";
    // 这里用单引号比较: 双引号加井号的字面量会提前终止外层的原始字符串。
    if (!raw || raw.charAt(0) === '#') return;
    if (/^(mailto|tel|javascript):/i.test(raw)) return;
    // 书源仓库的「一键导入」是自定义协议。系统浏览器不认识它们, 交出去只会
    // 静默失败, 必须在面板里截下并转给宿主。
    var importTarget = importProtocolUrl(raw);
    if (importTarget) {{
      event.preventDefault();
      requestImport(importTarget);
      return;
    }}
    var url;
    try {{ url = realUrl(raw); }} catch (error) {{ return; }}
    if (!/^https?:$/.test(url.protocol)) return;
    // 面板自身 URL(同文档跳转/桥导航)不拦截, 保持原有行为。
    if (isPanelUrl(url)) return;
    if (pageUrl) {{
      var current = new URL(pageUrl);
      if (current.origin === url.origin && current.pathname === url.pathname && current.search === url.search) return;
    }}
    // 现代 WebView 的 `window.navigation` 已经处理过这次跳转; 这里再拦一次
    // 会重复导航。只接管它没管的 `target="_blank"` 与老 WebView 的普通点击。
    if (window.navigation && typeof window.navigation.addEventListener === "function") {{
      var anchorTarget = (anchor.getAttribute("target") || "").toLowerCase();
      if (anchorTarget !== "_blank") return;
    }}
    event.preventDefault();
    try {{
      // 留在面板内跳转: iframe 的 src 变成代理地址, 页面由后端重写后渲染。
      window.location.assign(proxyUrl(url));
    }} catch (error) {{
      postToHost({{ type: "navigationError", message: "网页跳转失败，请重新加载后重试：" + String(error) }});
    }}
  }}, true);
  ["pushState", "replaceState"].forEach(function (method) {{
    var native = window.history[method];
    window.history[method] = function (state, title, url) {{
      if (arguments.length < 3 || url == null || !/^https?:/.test(pageUrl)) return native.apply(window.history, arguments);
      var target = realUrl(String(url));
      if (target.origin !== new URL(pageUrl).origin) throw new DOMException("History URL must stay on the page origin", "SecurityError");
      var result = native.call(window.history, state, title, proxyUrl(target));
      updatePageUrl(target);
      return result;
    }};
  }});
  window.addEventListener("popstate", function () {{
    var target = realUrl(window.location.href);
    if (/^https?:$/.test(target.protocol)) updatePageUrl(target);
  }});
  window.addEventListener("hashchange", function () {{
    if (!pageUrl) return;
    var target = new URL(pageUrl);
    target.hash = window.location.hash;
    updatePageUrl(target);
  }});

  // 面板内 console 转发到宿主日志, 便于排查
  async function bridgeAsync(action, payload, signal) {{
    var response = await nativeFetch(bridgeBase, {{
      method: "POST", headers: {{ "Content-Type": "text/plain;charset=UTF-8" }},
      body: JSON.stringify({{ action: action, payload: payload || {{}} }}), signal: signal
    }});
    if (!response.ok) throw new Error("bridge HTTP " + response.status);
    var result = await response.json();
    if (!result.ok) throw new Error(result.error || "书源操作失败");
    return result.value;
  }}

  // Inline settings pages use the source's native requests for CORS and cookies.
  window.fetch = async function (input, init) {{
    var rawUrl = input instanceof Request ? input.url : String(input);
    var url = new URL(rawUrl, pageUrl || window.location.href);
    if (!/^https?:$/.test(url.protocol) || url.origin === window.location.origin) return nativeFetch(input, init);
    var request = new Request(url.href, input instanceof Request ? input : init);
    if (input instanceof Request && init) request = new Request(request, init);
    if (input instanceof Request || init && (init.body instanceof FormData || init.body instanceof Blob || init.body instanceof ArrayBuffer || ArrayBuffer.isView(init.body))) {{
      // Preserve multipart boundaries and binary bodies through the raw proxy;
      // relative form submissions must resolve against the upstream page.
      var sameOrigin = pageUrl && url.origin === new URL(pageUrl).origin;
      if (!sameOrigin) return nativeFetch(request);
      if (init && init.body != null) {{
        // 用原始 BodyInit 重建, 而不是把已构造的 Request 再包一层: 后者会让请求体
        // 变成 ReadableStream, 经自定义协议发送时可能送不出体(后端于是收不到任何
        // 表单字段, 某聚合源的注册接口回「无效的操作」)。原始 FormData/Blob 重建则由
        // 浏览器在发送时按真实字节生成 Content-Type 与 boundary。
        return nativeFetch(new Request(proxyUrl(url), init));
      }}
      return nativeFetch(new Request(proxyUrl(url), request));
    }}
    var headers = {{}};
    request.headers.forEach(function (value, name) {{ headers[name] = value; }});
    var body = /^(GET|HEAD)$/i.test(request.method) ? "" : await request.text();
    var response = await bridgeAsync("fetch", {{ url: url.href, method: request.method, headers: headers, body: body }}, request.signal);
    if (!response.code) throw new Error(response.error || "网络请求失败");
    var responseHeaders = new Headers(response.headers || {{}});
    ["set-cookie", "content-encoding", "content-length"].forEach(function (name) {{ responseHeaders.delete(name); }});
    return new Response([204, 205, 304].includes(response.code) || request.method === "HEAD" ? null : response.body, {{ status: response.code, headers: responseHeaders }});
  }};

  var rawLog = console.log.bind(console);
  console.log = function () {{
    var parts = Array.prototype.slice.call(arguments).map(function (v) {{
      return typeof v === "string" ? v : JSON.stringify(v);
    }});
    bridgeSync("log", {{ message: parts.join(" ") }});
    rawLog.apply(null, arguments);
  }};
  window.onerror = function (msg, src, line) {{
    bridgeSync("log", {{ message: "面板 JS 错误: " + msg + " @" + line }});
  }};

  // 后端把 headers 当 JSON 文本解析(见 parser/js.rs 的 run_panel_request),
  // 而插件习惯写 java.get(url, {{'Referer': ...}}) 传对象, 直接透传会被后端的
  // 字符串取值丢掉, 表现为自定义头静默失效。这里统一序列化成文本。
  function hdrJson(h) {{
    if (h === null || h === undefined) return "";
    if (typeof h === "string") return h;
    try {{ return JSON.stringify(h); }} catch (e) {{ return ""; }}
  }}

  window.java = {{
    ajax: function (spec) {{
      var r = bridgeSync("ajax", {{ spec: String(spec) }});
      return r.ok ? r.value : "";
    }},
    get: function (url, headers) {{
      var r = bridgeSync("http", {{ method: "GET", url: String(url), body: "", headers: hdrJson(headers) }});
      return r.ok ? r.value : "";
    }},
    post: function (url, body, headers) {{
      var r = bridgeSync("http", {{ method: "POST", url: String(url), body: String(body || ""), headers: hdrJson(headers) }});
      return r.ok ? r.value : "";
    }},
    connect: function (url, headers) {{
      var r = bridgeSync("connect", {{ spec: String(url), headers: hdrJson(headers) }});
      return r.ok ? r.value : "";
    }},
    log: function (m) {{ bridgeSync("log", {{ message: String(m) }}); return m; }},
    toast: function (m) {{ postToHost({{ type: "toast", message: String(m) }}); }},
    longToast: function (m) {{ postToHost({{ type: "toast", message: String(m) }}); }},
    upConfig: function (config) {{
      try {{ postToHost({{ type: "config", options: typeof config === "string" ? JSON.parse(config) : config }}); }} catch (e) {{}}
    }},
    upLoginData: function (values) {{ postToHost({{ type: "loginData", values: values }}); }},
    reLoginView: function () {{ postToHost({{ type: "refreshLogin" }}); }},
    getCache: function (k) {{
      var r = bridgeSync("cacheGet", {{ key: String(k) }});
      return r.ok ? (r.value === undefined ? null : r.value) : null;
    }},
    putCache: function (k, v) {{ bridgeSync("cachePut", {{ key: String(k), value: String(v) }}); return true; }},
    // 面板需要与书源规则相同的加密能力(书源用它给接口签名)。
    // 缺了这些方法插件会降级到 CryptoJS 回退, 那条路径依赖注入 <script>, 容易失败。
    createSymmetricCrypto: function (transformation, key, iv) {{
      var t = String(transformation || "");
      var k = String(key == null ? "" : key);
      var v = String(iv == null ? "" : iv);
      function run(op, data, format) {{
        var r = bridgeSync("crypto", {{
          op: op, transformation: t, key: k, iv: v,
          data: String(data == null ? "" : data), format: format
        }});
        return r.ok ? String(r.value) : "";
      }}
      return {{
        encrypt: function (d) {{ return run("enc", d, "base64"); }},
        encryptBase64: function (d) {{ return run("enc", d, "base64"); }},
        encryptHex: function (d) {{ return run("enc", d, "hex"); }},
        decrypt: function (d) {{ return run("dec", d, "auto"); }},
        decryptStr: function (d) {{ return run("dec", d, "auto"); }},
        setIv: function (next) {{ v = String(next == null ? "" : next); return this; }}
      }};
    }},
    aesEncodeToBase64String: function (d, k, t, iv) {{
      return window.java.createSymmetricCrypto(t, k, iv).encryptBase64(d);
    }},
    aesEncodeToString: function (d, k, t, iv) {{
      return window.java.createSymmetricCrypto(t, k, iv).encryptHex(d);
    }},
    aesDecodeToString: function (d, k, t, iv) {{
      return window.java.createSymmetricCrypto(t, k, iv).decryptStr(d);
    }},
    desEncodeToBase64String: function (d, k, t, iv) {{
      return window.java.createSymmetricCrypto(t, k, iv).encryptBase64(d);
    }},
    desEncodeToString: function (d, k, t, iv) {{
      return window.java.createSymmetricCrypto(t, k, iv).encryptHex(d);
    }},
    desDecodeToString: function (d, k, t, iv) {{
      return window.java.createSymmetricCrypto(t, k, iv).decryptStr(d);
    }},
    tripleDESEncodeBase64Str: function (d, k, m, p, iv) {{
      return window.java.createSymmetricCrypto("DESede/" + m + "/" + p, k, iv).encryptBase64(d);
    }},
    tripleDESDecodeStr: function (d, k, m, p, iv) {{
      return window.java.createSymmetricCrypto("DESede/" + m + "/" + p, k, iv).decryptStr(d);
    }},
    // 面板里会检测这些哈希/编码方法是否存在
    md5Encode: function (v) {{ var r = bridgeSync("digest", {{ kind: "md5", input: String(v) }}); return r.ok ? String(r.value) : ""; }},
    base64Encode: function (v) {{ var r = bridgeSync("digest", {{ kind: "base64", input: String(v) }}); return r.ok ? String(r.value) : ""; }},
    base64Decode: function (v) {{ var r = bridgeSync("digest", {{ kind: "unbase64", input: String(v) }}); return r.ok ? String(r.value) : ""; }},
    // 面板内再次弹层: 后端登记为新面板, 由桥通知宿主切换
    showBrowser: function (url, html, preloadJs, options) {{
      var r = bridgeSync("showBrowser", {{ url: String(url), html: html == null ? null : String(html), preloadJs: preloadJs == null ? null : String(preloadJs), options: options }});
      if (r.ok && r.value) {{
        postToHost({{ type: "showBrowser", request: r.value }});
      }}
      return r.ok;
    }}
  }};

  function sourceGet(key) {{ var r = bridgeSync("sourceGet", {{ key: key }}); return r.ok ? r.value : null; }}
  function sourceSet(key, value) {{ return bridgeSync("sourceSet", {{ key: key, value: value == null ? null : String(value) }}).ok; }}
  function sourceMap(key) {{ try {{ return JSON.parse(sourceGet(key) || "{{}}"); }} catch (e) {{ return {{}}; }} }}
  window.source = {{
    getKey: function () {{ return window.__readerSourceKey || ""; }},
    getVariable: function () {{ return sourceGet("variable") || ""; }},
    setVariable: function (v) {{ return sourceSet("variable", v); }},
    putVariable: function (v) {{ return sourceSet("variable", v); }},
    getLoginInfo: function () {{ return sourceGet("loginInfo"); }},
    getLoginInfoMap: function () {{ return sourceMap("loginInfo"); }},
    putLoginInfo: function (v) {{ return sourceSet("loginInfo", v); }},
    removeLoginInfo: function () {{ return sourceSet("loginInfo", null); }},
    getLoginHeader: function () {{ return sourceGet("loginHeader"); }},
    getLoginHeaderMap: function () {{ return sourceMap("loginHeader"); }},
    putLoginHeader: function (v) {{ return sourceSet("loginHeader", typeof v === "string" ? v : JSON.stringify(v)); }},
    removeLoginHeader: function () {{ return sourceSet("loginHeader", null); }},
    get: function (k) {{ return window.java.getCache(k) || ""; }},
    put: function (k, v) {{ return window.java.putCache(k, v); }}
  }};

  window.cache = {{
    get: function (k) {{ return window.java.getCache(k); }},
    put: function (k, v) {{ return window.java.putCache(k, v); }},
    getFromMemory: function (k) {{ return window.java.getCache(k); }},
    putMemory: function (k, v) {{ return window.java.putCache(k, v); }},
    // 插件用它缓存 crypto-js 等大体积库(避免每次同步拉 CDN)
    getFile: function (k) {{
      var r = bridgeSync("cacheGet", {{ key: "file:" + String(k) }});
      return r.ok ? (r.value === undefined || r.value === null ? "" : r.value) : "";
    }},
    putFile: function (k, v) {{
      bridgeSync("cachePut", {{ key: "file:" + String(k), value: String(v) }});
      return v;
    }},
    "delete": function (k) {{ bridgeSync("cachePut", {{ key: String(k), value: "" }}); }}
  }};

  // window.run(script): 在宿主书源 jsLib 作用域里执行脚本, 返回 Promise
  window.run = async function (script) {{
    var value = await bridgeAsync("run", {{ script: String(script) }});
    if (value.showBrowser) postToHost({{ type: "showBrowser", request: value.showBrowser }});
    var effects = value.loginEffects || {{}};
    if (effects.reset) postToHost({{ type: "loginData", values: null }});
    if (effects.values && Object.keys(effects.values).length) postToHost({{ type: "loginData", values: effects.values }});
    if (effects.refresh) postToHost({{ type: "refreshLogin" }});
    (value.logs || []).forEach(function (line) {{ if (line.startsWith("[toast]")) postToHost({{ type: "toast", message: line.slice(7).trim() }}); }});
    return value.result;
  }};

  window.java.run = window.run;
  window.ajaxAwait = function (url) {{ return bridgeAsync("ajax", {{ spec: String(url) }}); }};

  function snapshotDocument() {{
    var copy = document.documentElement.cloneNode(true);
    var live = document.querySelectorAll("input,textarea,select");
    var cloned = copy.querySelectorAll("input,textarea,select");
    live.forEach(function (field, index) {{
      var target = cloned[index];
      if (field.tagName === "TEXTAREA") target.textContent = field.value;
      else if (field.tagName === "SELECT") Array.from(target.options).forEach(function (option, i) {{ option.toggleAttribute("selected", field.options[i].selected); }});
      else {{ target.setAttribute("value", field.value); target.toggleAttribute("checked", field.checked); }}
    }});
    return "<!DOCTYPE html>" + copy.outerHTML;
  }}
window.addEventListener("message", function (event) {{
    if (event.source !== window.parent || !event.data || event.data.panelId !== panelId) return;
    if (event.data.type === "reader:ready") {{
      if (!navigating) ready(event.data.requestId);
      return;
    }}
    if (event.data.type !== "reader:confirm" || event.data.documentId !== documentId) return;
    var requestId = event.data.requestId;
    if (navigating) {{
      postToHost({{ type: "error", requestId: requestId, message: "网页正在跳转，请加载完成后再确认" }});
      return;
    }}
    var r;
    try {{ r = bridgeSync("complete", {{ url: pageUrl, body: snapshotDocument() }}); }}
    catch (error) {{ r = {{ ok: false, error: String(error) }}; }}
    postToHost(r.ok ? {{ type: "complete", requestId: requestId }} : {{ type: "error", requestId: requestId, message: r.error }});
  }});
  function ready(requestId) {{ postToHost({{ type: "ready", requestId: requestId, title: document.title, url: pageUrl }}); }}
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", function () {{ ready(); }}, {{ once: true }});
  else ready();

  // 插件用 window.close() 关闭面板。
  // iframe 内 window.close() 通常无效(浏览器只允许关脚本自开的窗口),
  // 因此先通知宿主关掉弹层, 再尝试原生 close。
  window.close = function () {{
    postToHost({{ type: "close" }});
  }};
}})();
</script>"#
    )
}

/// 登录页请求代理脚本。Cookie 不暴露给页面，避免不同书源共享
/// reader.localhost origin。
pub(super) fn build_cookie_harvest_script(book_source_url: &str, login_session: &str) -> String {
    let login_session_json =
        serde_json::to_string(login_session).unwrap_or_else(|_| "\"\"".to_string());
    // 登录页跑在 reader origin 的**上游原始路径**上(见前端 frameUrl), 页面发出的
    // 每个请求天然带着正确路径, 由兜底路由统一转发回上游。
    //
    // 因此这里**不再改写** fetch/XHR/资源地址: 一旦改写成 `/bookSourceProxy?...`,
    // 打包产物的绝对路径(路由 base `/user/`、分片 `"/user/"+chunk`)就会全部 404,
    // 单页应用永远挂载不起来 —— 那正是登录页长期白屏的根因。保持地址原样才能
    // 复刻 legado 在手机端直接 `loadUrl` 的行为。
    //
    // 本脚本只负责把 document.cookie 的变化同步给宿主(部分站点把凭据写在脚本
    // 可读的 cookie 里), 供上层尝试采集。
    let _ = book_source_url;
    format!(
        r#"<script>
(function() {{
  var loginSession = {login_session_json};
  function report() {{
    try {{
      if (window.parent && window.parent !== window) {{
        window.parent.postMessage(
          {{ type: "reader:loginCookies", loginSession: loginSession, cookie: String(document.cookie || "") }},
          "*"
        );
      }}
    }} catch (_e) {{}}
  }}
  report();
  document.addEventListener("visibilitychange", function () {{
    if (document.visibilityState === "hidden") report();
  }});
  window.addEventListener("pagehide", report);
}})();
</script>
"#,
        login_session_json = login_session_json,
    )
}

/// 从主机名取可注册域(registrable domain), 用于判断两个主机是否属同一站点。
///
/// 注意这是**近似实现**: 只覆盖常见的多标签公共后缀, 不读取完整的 Public
/// Suffix List(`publicsuffix` crate 是动态列表, 需要额外的数据源, 与这里
/// 「不引入运行时下载依赖」的前提冲突)。表外的多标签后缀会被当成普通域名,
/// 例如 `www.abc.co.nz` 与 `evil.co.nz` 都会被折叠成 `co.nz` 而**误判为同站** ——
/// 因此调用方不得把「同站」当作安全边界, 见 `set_panel_cookie` 的说明。
pub(super) fn registrable_domain_from_host(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let parts: Vec<&str> = host.split('.').filter(|part| !part.is_empty()).collect();
    if parts.len() < 2 {
        return host;
    }
    // 常见多标签公共后缀。表越长越准, 但**永远不可能完备**, 这是本函数近似的根源。
    const TWO_LABEL_SUFFIXES: &[&str] = &[
        "co.uk", "org.uk", "me.uk", "ac.uk", "gov.uk", "net.uk", "sch.uk",
        "com.cn", "net.cn", "org.cn", "gov.cn", "edu.cn", "ac.cn",
        "com.au", "net.au", "org.au", "edu.au", "gov.au",
        "co.jp", "ne.jp", "or.jp", "ac.jp", "go.jp",
        "co.kr", "or.kr", "ne.kr", "go.kr", "re.kr",
        "com.br", "net.br", "org.br", "gov.br",
        "com.hk", "org.hk", "net.hk", "edu.hk", "gov.hk",
        "com.tw", "org.tw", "net.tw", "edu.tw", "gov.tw",
        "co.in", "net.in", "org.in", "gen.in", "firm.in",
        "co.nz", "net.nz", "org.nz", "govt.nz", "ac.nz",
        "com.sg", "net.sg", "org.sg", "edu.sg", "gov.sg",
        "com.my", "net.my", "org.my", "edu.my", "gov.my",
        "co.id", "or.id", "ac.id", "go.id", "web.id",
        "co.th", "or.th", "ac.th", "go.th", "in.th",
        "co.za", "net.za", "org.za", "gov.za", "ac.za",
        "com.ar", "net.ar", "org.ar", "gov.ar", "edu.ar",
        "com.mx", "net.mx", "org.mx", "gob.mx", "edu.mx",
        "com.tr", "net.tr", "org.tr", "gov.tr", "edu.tr",
        "co.il", "org.il", "net.il", "ac.il", "gov.il",
        "com.ua", "net.ua", "org.ua", "gov.ua", "edu.ua",
        "com.vn", "net.vn", "org.vn", "gov.vn", "edu.vn",
        "co.jp", "com.ph", "com.pk", "com.eg", "com.sa",
        "com.co", "com.pe", "com.ve", "com.uy", "com.ec",
        "co.ke", "co.tz", "co.ug", "co.zw", "co.bw",
    ];
    let suffix = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    if parts.len() >= 3 && TWO_LABEL_SUFFIXES.contains(&suffix.as_str()) {
        return parts[parts.len() - 3..].join(".");
    }
    parts[parts.len() - 2..].join(".")
}
