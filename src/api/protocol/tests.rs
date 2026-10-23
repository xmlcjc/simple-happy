use super::*;

    /// 路由表的静态断言：拆分/移动本文件时，任何一个已知路径漏注册都会在这里失败。
    #[test]
    fn every_known_protocol_path_has_its_own_route() {
        let cases = [
            ("/bookSourceProxy", RouteKind::BookSourceProxy),
            ("/sourcePanelBridge", RouteKind::SourcePanelBridge),
            ("/sourcePanel", RouteKind::SourcePanel),
            ("/cover", RouteKind::Cover),
            ("/epub", RouteKind::Epub),
            ("/pdf", RouteKind::Pdf),
            ("/files", RouteKind::Files),
            ("/musicStream", RouteKind::MusicStream),
            ("/musicLocal", RouteKind::MusicLocal),
            ("/__login/abc123/user/me", RouteKind::LoginProxy),
            ("/__login/", RouteKind::LoginProxy),
            // 前缀必须带上结尾斜杠: `/__log` 不属于登录页空间。
            ("/__log", RouteKind::Fallback),
            ("/", RouteKind::Fallback),
            ("/unknown/path", RouteKind::Fallback),
        ];
        for (path, expected) in cases {
            assert_eq!(classify_route(path), expected, "path = {path}");
        }
    }

    #[tokio::test]
    #[ignore = "reads an explicitly supplied source export and its public registration page"]
    async fn live_source_registration_page() {
        let path = std::env::var("READER_REGISTRATION_SOURCE_FILE").expect("source JSON path");
        let value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let source = crate::model::book_source::book_source_from_value(value).unwrap();
        let target = std::env::var("READER_REGISTRATION_URL").expect("public registration URL");
        let http = crate::crawler::http_client::HttpClient::new(20, None).unwrap();
        let context = http.source_context("registration-smoke", &source.book_source_url, None).unwrap();
        let entry = crate::service::book_service::SourcePanelEntry {
            url: target.clone(), book_source_url: source.book_source_url.clone(), http: Some(context), ..Default::default()
        };
        let response = fetch_source_panel_response(&source, "fixture-settings", &entry, &target, &http::Request::new(Vec::new())).await.unwrap();
        assert_eq!(response.status(), 200);
        let html = String::from_utf8(response.into_body()).unwrap();
        assert!(html.contains("reg-email"));
        assert!(html.contains("window.java ="));
        std::fs::create_dir_all("target/source-panel-fixtures").unwrap();
        std::fs::write("target/source-panel-fixtures/registration.html", html).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panel_pages_and_form_posts_evaluate_dynamic_source_headers() {
        use axum::{body::Bytes, http::HeaderMap, routing::get, Router};
        use crate::crawler::http_client::HttpClient;
        use crate::service::book_service::SourcePanelEntry;

        let app = Router::new().route("/register", get(|headers: HeaderMap| async move {
            assert_eq!(headers.get("user-agent").unwrap(), "Legado/3.0");
            assert!(headers.get("authorization").is_none());
            ([("content-type", "text/html")], "<html><head><title>注册</title></head><body><input id=\"reg-email\"><script src=\"/static/auth.js\"></script></body></html>")
        }).post(|headers: HeaderMap, body: Bytes| async move {
            assert_eq!(headers.get("authorization").unwrap(), "Bearer fixture-token");
            assert_eq!(headers.get("content-type").unwrap(), "multipart/form-data; boundary=fixture");
            assert_eq!(body.as_ref(), b"--fixture\r\nContent-Disposition: form-data; name=\"email\"\r\n\r\nreader@example.test\r\n--fixture--\r\n");
            ([("content-type", "application/json")], r#"{"code":200}"#)
        })).route("/static/auth.js", get(|| async { ([("content-type", "text/javascript")], "window.registrationReady = true;") }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let source = BookSource {
            book_source_url: format!("custom://rapid/{}", uuid::Uuid::new_v4()),
            header: Some("@js:var v=JSON.parse(source.getVariable()||'{}');JSON.stringify(v.token?{'Authorization':'Bearer '+v.token,'User-Agent':'Legado/3.0'}:{'User-Agent':'Legado/3.0'})".into()),
            ..Default::default()
        };
        let http = HttpClient::new(3, None).unwrap();
        let context = http.source_context("panel-registration", &source.book_source_url, None).unwrap();
        let entry = SourcePanelEntry { url: format!("{base}/register"), book_source_url: source.book_source_url.clone(), http: Some(context.clone()), ..Default::default() };
        let response = fetch_source_panel_response(&source, "registration", &entry, &entry.url, &http::Request::new(Vec::new())).await.unwrap();
        assert_eq!(response.status(), 200);
        let html = String::from_utf8(response.into_body()).unwrap();
        assert!(html.contains("reg-email"));
        assert!(html.contains("window.java ="));
        assert!(html.contains("__readerPanelUrl="));
        let script = fetch_source_panel_response(&source, "registration", &entry, &format!("{base}/static/auth.js"), &http::Request::new(Vec::new())).await.unwrap();
        assert_eq!(script.body(), b"window.registrationReady = true;");
        context.session.put("variable", Some(r#"{"token":"fixture-token"}"#)).unwrap();
        let request = http::Request::builder().method("POST")
            .header("content-type", "multipart/form-data; boundary=fixture")
            .body(b"--fixture\r\nContent-Disposition: form-data; name=\"email\"\r\n\r\nreader@example.test\r\n--fixture--\r\n".to_vec()).unwrap();
        let response = fetch_source_panel_response(&source, "registration", &entry, &entry.url, &request).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), br#"{"code":200}"#);
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panel_form_post_infers_missing_content_type() {
        // 某聚合源的注册页用 FormData 提交(send_code), WebView2 交给宿主的自定义
        // 协议请求可能不带 Content-Type。后端(PHP)缺该头时完全不解析请求体,
        // 表现为注册接口回「无效的操作」。这里断言转发时按体的真实形态补齐,
        // 且 multipart 的 boundary 取自体本身(不能一律按 urlencoded 兜底, 那会
        // 把 multipart 体错标, 照样解析不出字段)。
        use axum::{body::Bytes, http::HeaderMap, routing::post, Router};
        use crate::crawler::http_client::HttpClient;
        use crate::service::book_service::SourcePanelEntry;

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let sink = seen.clone();
        let app = Router::new().route("/register_api", post(move |headers: HeaderMap, body: Bytes| {
            let sink = sink.clone();
            async move {
                sink.lock().unwrap().push(format!(
                    "{}|{}",
                    headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("<none>"),
                    String::from_utf8_lossy(&body)
                ));
                ([("content-type", "application/json")], r#"{"success":false}"#)
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let source = BookSource { book_source_url: base.clone(), ..Default::default() };
        let http = HttpClient::new(3, None).unwrap();
        let context = http.source_context("panel-ct", &source.book_source_url, None).unwrap();
        let entry = SourcePanelEntry {
            url: format!("{base}/register_api"),
            book_source_url: source.book_source_url.clone(),
            http: Some(context.clone()),
            ..Default::default()
        };
        let target = format!("{base}/register_api");

        // 1. multipart 体, 无 Content-Type -> 必须推断出体的 boundary
        let multipart = b"--fixture\r\nContent-Disposition: form-data; name=\"action\"\r\n\r\nsend_code\r\n--fixture--\r\n".to_vec();
        let request = http::Request::builder().method("POST").body(multipart).unwrap();
        let response = fetch_source_panel_response(&source, "ct", &entry, &target, &request).await.unwrap();
        assert_eq!(response.status(), 200);

        // 2. urlencoded 体, 无 Content-Type -> 兜底为 urlencoded
        let urlencoded = b"action=send_code&email=reader%40example.test".to_vec();
        let request = http::Request::builder().method("POST").body(urlencoded).unwrap();
        fetch_source_panel_response(&source, "ct", &entry, &target, &request).await.unwrap();

        // 3. 显式给定且与体一致(同 boundary)的 Content-Type 原样保留
        let request = http::Request::builder().method("POST")
            .header("content-type", "multipart/form-data; boundary=explicit")
            .body(b"--explicit\r\nContent-Disposition: form-data; name=\"action\"\r\n\r\nsend_code\r\n--explicit--\r\n".to_vec())
            .unwrap();
        fetch_source_panel_response(&source, "ct", &entry, &target, &request).await.unwrap();

        // 4. 头与体不符(multipart 体却声明 urlencoded) -> 必须按体重写,
        //    否则后端照样解析不出字段(实测即「无效的操作」)
        let request = http::Request::builder().method("POST")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(b"--mismatch\r\nContent-Disposition: form-data; name=\"action\"\r\n\r\nsend_code\r\n--mismatch--\r\n".to_vec())
            .unwrap();
        fetch_source_panel_response(&source, "ct", &entry, &target, &request).await.unwrap();

        // 5. 非 multipart 体带显式头 -> 不覆盖调用方给的头
        let request = http::Request::builder().method("POST")
            .header("content-type", "application/json")
            .body(br#"{"action":"send_code"}"#.to_vec())
            .unwrap();
        fetch_source_panel_response(&source, "ct", &entry, &target, &request).await.unwrap();

        let recorded = seen.lock().unwrap().clone();
        assert_eq!(recorded.len(), 5);
        assert!(recorded[0].starts_with("multipart/form-data; boundary=fixture|"), "缺头时必须从体推断 boundary: {}", recorded[0]);
        assert!(recorded[0].contains("action"), "体必须完整转发: {}", recorded[0]);
        assert!(recorded[1].starts_with("application/x-www-form-urlencoded"), "非 multipart 体按 urlencoded 兜底: {}", recorded[1]);
        assert!(recorded[2].starts_with("multipart/form-data; boundary=explicit|"), "与体一致的头不得被改写: {}", recorded[2]);
        assert!(recorded[3].starts_with("multipart/form-data; boundary=mismatch|"), "头与体不符时必须按体重写: {}", recorded[3]);
        assert!(recorded[4].starts_with("application/json|"), "非 multipart 体的显式头不得被覆盖: {}", recorded[4]);
        server.abort();
    }

    #[test]
    fn multipart_boundary_is_taken_from_body() {
        // 体的首行即 `--boundary`; 非 multipart 体不得被误判。
        assert_eq!(
            multipart_boundary(b"--fixture\r\nContent-Disposition: form-data\r\n").as_deref(),
            Some("fixture")
        );
        assert_eq!(
            multipart_boundary(b"--WebKitFormBoundaryABC123\n").as_deref(),
            Some("WebKitFormBoundaryABC123")
        );
        assert_eq!(multipart_boundary(b"action=send_code&email=x"), None);
        assert_eq!(multipart_boundary(b""), None);
        // 只有 `--` 没有内容 / 含空白与控制字符 -> 不是合法 boundary
        assert_eq!(multipart_boundary(b"--\r\n"), None);
        assert_eq!(multipart_boundary(b"--bad boundary\r\n"), None);
        assert_eq!(multipart_boundary(b"--bad\tboundary\r\n"), None);
        // 超长(>70)不是合法 boundary
        let long = format!("--{}\r\n", "a".repeat(71));
        assert_eq!(multipart_boundary(long.as_bytes()), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panel_cookie_writes_reach_source_jar() {
        // 某聚合源的登录页用 `document.cookie = 'user_token=…'` 记录会话, 而 /user_center
        // 由**服务端**读 Cookie 判断登录态。只写浏览器的话这份 cookie 留在面板
        // origin(reader.localhost)下, 转发请求带不上, 上游就把页面踢回登录页
        // (用户看到「登录过期」)。这里断言: 面板页写入的 cookie 能落到书源 jar,
        // 从而让后续转发请求带上它。
        use axum::{http::HeaderMap, routing::get, Router};
        use crate::crawler::http_client::HttpClient;
        use crate::service::book_service::SourcePanelEntry;

        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = seen.clone();
        let app = Router::new().route("/user_login", get(move |headers: HeaderMap| async move {
            *sink.lock().unwrap() = headers.get("cookie").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            ([("content-type", "text/html")], "<html><head></head><body>登录</body></html>")
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let source = BookSource { book_source_url: base.clone(), ..Default::default() };
        let http = HttpClient::new(3, None).unwrap();
        let context = http.source_context("panel-cookie-write", &source.book_source_url, None).unwrap();
        let entry = SourcePanelEntry {
            url: format!("{base}/user_login"),
            book_source_url: source.book_source_url.clone(),
            http: Some(context.clone()),
            ..Default::default()
        };
        // 桥按 panelId 从全局注册表取面板, 因此必须登记。
        let panel_id = crate::service::book_service::register_source_panel(entry.clone());

        // jar 为空时也必须注入劫持脚本 —— 登录前 jar 正是空的, 而回写方向与
        // jar 是否为空无关; 少了这一步页面写入的 cookie 就永远进不了 jar。
        let response = fetch_source_panel_response(&source, &panel_id, &entry, &entry.url, &http::Request::new(Vec::new())).await.unwrap();
        let html = String::from_utf8(response.into_body()).unwrap();
        assert!(html.contains("Object.defineProperty(document, 'cookie'"), "jar 为空时也要注入劫持脚本");
        assert!(html.contains("__readerPanelQueueCookie"), "setter 必须把写入排队回传: {}", html.chars().take(300).collect::<String>());
        assert!(html.contains("__readerPanelBridgeReady"), "桥应置就绪标志以触发补发");
        assert!(html.contains("__readerPanelCookieFlush"), "桥应触发排队项的补发");

        // 首次请求 jar 里没有 cookie
        assert!(!seen.lock().unwrap().contains("user_token"), "首次请求不应带 cookie");

        // 走真实的桥动作提交 cookie(面板页写入 document.cookie 后桥走同步 XHR)。
        // 这里直接调用 cookieSet 的实现函数 —— 该分支不使用 book_source_service,
        // 因此可以只验证 cookie 落 jar 与越域拦截这两条逻辑。
        let bridge_set = |url: &str, cookie: &str| {
            crate::service::book_service::set_panel_cookie(&entry, url, cookie)
        };
        bridge_set(&entry.url, "user_token=token-abc123").expect("cookieSet 应成功写入 jar");

        // 再次请求: 转发必须带上刚才写入的 cookie
        fetch_source_panel_response(&source, &panel_id, &entry, &entry.url, &http::Request::new(Vec::new())).await.unwrap();
        assert!(seen.lock().unwrap().contains("user_token=token-abc123"), "写入 jar 的 cookie 应随转发请求送达: {}", seen.lock().unwrap());

        // 越域的 cookie 目标必须被拒绝(不能把会话写到书源域之外)
        assert!(bridge_set("https://evil.example.com/", "user_token=leak").is_err(), "跨域 cookie 目标必须被拒绝");
        assert!(!context.session.cookies.get(&url::Url::parse("https://evil.example.com/").unwrap()).contains("user_token"), "不得写入书源域之外的 jar");

        // 非法 cookie 串(无 `=`)不得写入
        assert!(bridge_set(&entry.url, "not-a-cookie").is_err(), "非法 cookie 应被拒绝");

        // 含控制字符的 cookie 不得写入(header 注入边界收紧在写入处)
        assert!(bridge_set(&entry.url, "evil=va\r\nlue").is_err(), "含 CRLF 的 cookie 应被拒绝");
        assert!(bridge_set(&entry.url, "ev\ril=value").is_err(), "名字含控制字符应被拒绝");

        // https 源不得把会话写进 http 目标(cookie 不区分 scheme, 明文请求会带上它)。
        // 注意本测试的 entry 是 http(本地 axum), 因此这里用 https 源单独验。
        let https_source = BookSource { book_source_url: "https://secure.example.test".to_string(), ..Default::default() };
        let https_http = HttpClient::new(3, None).unwrap();
        let https_context = https_http.source_context("panel-https", &https_source.book_source_url, None).unwrap();
        let https_entry = SourcePanelEntry {
            url: "https://secure.example.test/login".to_string(),
            book_source_url: https_source.book_source_url.clone(),
            http: Some(https_context.clone()),
            ..Default::default()
        };
        assert!(
            crate::service::book_service::set_panel_cookie(&https_entry, "http://secure.example.test/login", "user_token=x").is_err(),
            "https 源写入 http 目标必须被拒绝"
        );
        assert!(
            crate::service::book_service::set_panel_cookie(&https_entry, "https://secure.example.test/login", "user_token=x").is_ok(),
            "https 源写入 https 目标应放行"
        );

        // 空值代表页面删除 cookie(退出登录, 该页的 userLogout 即写 `user_token=;`
        // 配合过去时间)。必须从 jar 移除, 而不是留下空值发给上游。
        bridge_set(&entry.url, "user_token=").expect("删除应成功");
        fetch_source_panel_response(&source, &panel_id, &entry, &entry.url, &http::Request::new(Vec::new())).await.unwrap();
        assert!(!seen.lock().unwrap().contains("user_token"), "删除后的 cookie 不应再被转发: {}", seen.lock().unwrap());

        server.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panel_html_exposes_source_jar_cookies_to_page_scripts() {
        // 这类站点的前端用 document.cookie 判断登录态(getCookie('qttoken')),
        // 面板 HTML 响应需要把书源 jar 的 cookie 以 Set-Cookie 透出给面板 origin。
        use axum::{http::HeaderMap, routing::get, Router};
        use crate::crawler::http_client::HttpClient;
        use crate::service::book_service::SourcePanelEntry;

        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = seen.clone();
        let app = Router::new().route("/user", get(move |headers: HeaderMap| async move {
            *sink.lock().unwrap() = headers.get("cookie").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            ([("content-type", "text/html")], "<html><head><title>用户后台</title></head><body>用户中心</body></html>")
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let source = BookSource { book_source_url: format!("https://aggregate-{}.test", uuid::Uuid::new_v4()), ..Default::default() };
        let http = HttpClient::new(3, None).unwrap();
        let context = http.source_context("panel-cookie", &source.book_source_url, None).unwrap();
        let url = url::Url::parse(&format!("{base}/user")).unwrap();
        context.session.cookies.set(&url, "qttoken=localtoken123456789; deviceId=device-fixture").unwrap();
        let entry = SourcePanelEntry { url: url.to_string(), book_source_url: source.book_source_url.clone(), http: Some(context.clone()), ..Default::default() };

        let response = fetch_source_panel_response(&source, "user", &entry, &entry.url, &http::Request::new(Vec::new())).await.unwrap();
        assert_eq!(response.status(), 200);
        // 转发请求带上 jar cookie
        assert!(seen.lock().unwrap().contains("qttoken=localtoken123456789"), "上游应收到 jar 中的 qttoken");
        // 面板 origin 的 JS 可读到透出的 cookie（每个 cookie 对一个 Set-Cookie 头）
        let set_cookies: Vec<&str> = response.headers().get_all("set-cookie").iter()
            .filter_map(|v| v.to_str().ok()).collect();
        assert!(set_cookies.iter().any(|c| c.starts_with("qttoken=localtoken123456789;")), "应透出 qttoken: {set_cookies:?}");
        assert!(set_cookies.iter().any(|c| c.starts_with("deviceId=device-fixture;")), "应透出 deviceId: {set_cookies:?}");
        // 不依赖 WebView cookie 行为: HTML 内注入 document.cookie 劫持脚本,
        // jar cookie 以字面量嵌入（页面 getCookie('qttoken') 直接可读）
        let html = String::from_utf8(response.into_body()).unwrap();
        assert!(html.contains("jarCookie"), "应注入 jar cookie 变量");
        assert!(html.contains("qttoken=localtoken123456789"), "jar cookie 应嵌入注入脚本: {}", html.chars().take(200).collect::<String>());
        assert!(html.contains("deviceId=device-fixture"), "jar cookie 应完整嵌入(含 deviceId)");
        assert!(html.contains("Object.defineProperty(document, 'cookie'"), "应劫持 document.cookie");
        assert!(html.matches("<script>").count() >= 2, "注入脚本应在页面 head 中先于页面脚本执行");
        server.abort();
    }

    #[test]
    fn reader_scheme_origin_matches_platform() {
        let origin = reader_scheme_origin();
        if cfg!(any(target_os = "windows", target_os = "android")) {
            assert_eq!(origin, "http://reader.localhost");
        } else {
            assert_eq!(origin, "reader://localhost");
        }
    }

    #[test]
    fn panel_bridge_rejects_foreign_origins() {
        // 同源(浏览器同步 XHR 不带 Origin)放行
        assert!(panel_bridge_origin_allowed(None));
        // 面板自身 origin 放行
        assert!(panel_bridge_origin_allowed(Some(reader_scheme_origin())));
        // 外部网页一律拒绝: 否则任何本机页面都能借桥驱动宿主的网络/脚本能力
        assert!(!panel_bridge_origin_allowed(Some("https://evil.example")));
        assert!(!panel_bridge_origin_allowed(Some("http://tauri.localhost")));
        // 不能因为大小写/空白差异被绕过
        assert!(!panel_bridge_origin_allowed(Some("HTTP://EVIL.EXAMPLE")));
        assert!(panel_bridge_origin_allowed(Some(&format!(
            "  {}  ",
            reader_scheme_origin()
        ))));
    }

    #[test]
    fn panel_bridge_response_echoes_origin_without_wildcard() {
        let echoed = json_response_panel(
            200,
            serde_json::json!({ "ok": true }),
            Some(reader_scheme_origin()),
        );
        assert_eq!(
            echoed
                .headers()
                .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|v| v.to_str().ok()),
            Some(reader_scheme_origin())
        );
        // 同源请求不回 CORS 头, 更不能出现通配符
        let same_origin = json_response_panel(200, serde_json::json!({ "ok": true }), None);
        assert!(same_origin
            .headers()
            .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
    }


    #[test]
    fn registrable_domain_covers_common_multi_label_suffixes() {
        // 多标签公共后缀必须整体保留, 否则不同注册域会被误判为同站
        // (cookie 写入授权复用了这个判定, 误判即等于放开跨域写入)。
        for (host, expected) in [
            ("www.abc.co.nz", "abc.co.nz"),
            ("evil.co.nz", "evil.co.nz"),
            ("www.shop.com.sg", "shop.com.sg"),
            ("a.b.com.my", "b.com.my"),
            ("www.abc.gov.cn", "abc.gov.cn"),
            ("x.ac.uk", "x.ac.uk"),
            ("www.example.co.uk", "example.co.uk"),
            ("www.example.com.cn", "example.com.cn"),
            // 表外的普通域名仍折叠到末两段(近似行为, 已知不完备)
            ("a.b.example.com", "example.com"),
        ] {
            assert_eq!(registrable_domain_from_host(host), expected, "host={host}");
        }
        // co.nz 已在表内: 两个不同的注册域必须被区分开, 否则跨域 cookie 写入会被放行。
        assert_ne!(
            registrable_domain_from_host("www.abc.co.nz"),
            registrable_domain_from_host("evil.co.nz"),
            "不同注册域不得折叠成同一个值(否则跨域写入会被放行)"
        );
    }

    #[test]
    fn login_target_preserves_nested_jump_url_query() {
        let target = resolve_proxy_target_url(
            "https://alpha.example.com/login.php?jumpurl=https://alpha.example.com/",
            "https://alpha.example.com/",
        )
        .unwrap();
        assert_eq!(target, "https://alpha.example.com/login.php?jumpurl=https://alpha.example.com/");
        assert!(proxy_target_matches_source(&target, "https://alpha.example.com/"));
    }

    #[test]
    fn panel_outer_query_overrides_upstream_query() {
        // 某聚合源的登录页 `sw('register')` 把 mode 写进代理 URL 外层 query,
        // 初始加载已删掉 __readerPanelUrl(桥的 proxyUrl(url, storedHtml=true)),
        // 合并必须把 mode=register 送到上游, 否则页面永远是登录态。
        let params = parse_query("__readerPanelId=panel-1&mode=register");
        let merged = merge_panel_outer_query("https://v1.vossc.com/user_login", &params, true).unwrap();
        assert_eq!(merged, "https://v1.vossc.com/user_login?mode=register");
        // 与真实域名校验配合, 合并结果仍在书源域名内
        assert!(proxy_target_matches_source(&merged, "https://v1.vossc.com"));
    }

    #[test]
    fn panel_outer_query_preserves_other_upstream_params() {
        // 外层与内层同名时外层优先; 内层独有的参数保留。
        let params = parse_query("__readerPanelId=panel-1&mode=register&page=3");
        let merged = merge_panel_outer_query("https://example.com/list?mode=login&keep=1", &params, true).unwrap();
        // 用有序序列断言, 不用 HashMap 收集(query_pairs 收集到 HashMap 会天然去重, 掩盖重复项问题)。
        let pairs: Vec<(String, String)> = Url::parse(&merged).unwrap()
            .query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        assert!(pairs.contains(&("mode".into(), "register".into())));
        assert!(pairs.contains(&("keep".into(), "1".into())));
        assert!(pairs.contains(&("page".into(), "3".into())));
        // 被覆盖的旧值不得残留
        assert!(!pairs.contains(&("mode".into(), "login".into())));
    }

    #[test]
    fn panel_outer_query_replaces_every_duplicate_key() {
        // review P1: 外层 `a=3` 必须整体替换上游 `?a=1&a=2`, 得 `?a=3` 而不是
        // `?a=3&a=2` —— 后者会静默改变上游请求语义, 且与 JS 的 searchParams.set
        // (删全部同名再设一个)不一致。
        let params = parse_query("__readerPanelId=panel-1&a=3");
        let merged = merge_panel_outer_query("https://example.com/x?a=1&a=2", &params, true).unwrap();
        let pairs: Vec<(String, String)> = Url::parse(&merged).unwrap()
            .query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        assert_eq!(pairs, vec![("a".to_string(), "3".to_string())]);
    }

    #[test]
    fn panel_outer_query_leaves_identical_upstream_query_untouched() {
        // review P5/P6: 面板内导航的外层 query 常态是 proxyUrl 复制自上游。
        // 参数原样存在时必须返回 None, 让调用方沿用原 target, 不做
        // query_pairs 解码/重编码往返(否则 %20 变 +、%zz 变 %25zz, 上游按
        // query 原文签名时会失配), 也不因 HashMap 迭代序造成结果不稳定。
        let base = "https://example.com/x?q=a%20b%2Fc&sig=%zz&keep=1";
        let params = parse_query("__readerPanelId=panel-1&q=a%20b%2Fc&sig=%zz&keep=1");
        assert!(merge_panel_outer_query(base, &params, true).is_none());
    }

    #[test]
    fn panel_outer_query_skips_transport_and_untouched_cases() {
        // 只有传输字段 → 无需合并, 调用方沿用原 target(不改变既有行为)。
        let params = parse_query("__readerPanelId=panel-1&__readerPanelUrl=https%3A%2F%2Fexample.com%2Fx");
        assert!(merge_panel_outer_query("https://example.com/x", &params, true).is_none());
        // 非 namespaced 形态排除 url/panelId, 但上游自己的业务参数不可丢。
        let params = parse_query("panelId=panel-1&url=https%3A%2F%2Fexample.com%2Fx&mode=register");
        let merged = merge_panel_outer_query("https://example.com/x", &params, false).unwrap();
        assert_eq!(merged, "https://example.com/x?mode=register");
        // Non-http(s) 与 data: 面板不参与合并。
        let params = parse_query("__readerPanelId=panel-1&mode=register");
        assert!(merge_panel_outer_query("data:text/html,<p>x</p>", &params, true).is_none());
        // base 不是绝对 URL(相对路径且无 scheme)时返回 None。
        assert!(merge_panel_outer_query("/relative/path", &params, true).is_none());
    }

    #[test]
    fn panel_outer_query_appends_without_rewriting_existing_query() {
        // review P5: 纯追加场景(该页点注册)不得重建整串 query, 否则
        // `%20` 会被重编码成 `+`, 上游按 query 原文签名时会失配。
        let params = parse_query("__readerPanelId=panel-1&mode=register");
        let merged = merge_panel_outer_query("https://example.com/x?q=a%20b&sig=%zz", &params, true).unwrap();
        assert!(merged.contains("q=a%20b"), "既有片段的原文必须保留: {merged}");
        assert!(merged.contains("sig=%zz"), "非法百分号序列不得被改写: {merged}");
        assert!(merged.ends_with("&mode=register"), "新参数追加在末尾: {merged}");
    }

    #[test]
    fn panel_outer_query_has_stable_order_for_multiple_new_params() {
        // review P6: params 是 HashMap, 迭代序随机; 排序后追加顺序可复现。
        let params = parse_query("__readerPanelId=panel-1&zz=1&aa=2&mm=3");
        let first = merge_panel_outer_query("https://example.com/x", &params, true).unwrap();
        let second = merge_panel_outer_query("https://example.com/x", &params, true).unwrap();
        assert_eq!(first, second);
        assert_eq!(first, "https://example.com/x?aa=2&mm=3&zz=1");
    }

    /// 某聚合源的登录页的端到端链路: 初始加载(删掉 __readerPanelUrl)后页面 JS
    /// 改写外层 query 触发注册导航, 上游必须收到 mode=register 才能渲染注册态。
    #[test]
    fn panel_register_navigation_reaches_upstream_with_mode() {
        // 初始加载形态: proxyUrl(url, storedHtml=true) 会删掉 __readerPanelUrl
        let initial = "http://reader.localhost/sourcePanel?__readerPanelId=panel-1";
        let params = parse_query("__readerPanelId=panel-1");
        assert!(merge_panel_outer_query("https://v1.vossc.com/user_login", &params, true).is_none());
        // 页面 sw('register') 等价: 在 location.href 上加 mode 后整页导航
        let navigated = format!("{initial}&mode=register");
        let params = parse_query("__readerPanelId=panel-1&mode=register");
        let merged = merge_panel_outer_query("https://v1.vossc.com/user_login", &params, true).unwrap();
        assert_eq!(merged, "https://v1.vossc.com/user_login?mode=register");
        assert!(proxy_target_matches_source(&merged, "https://v1.vossc.com"));
        // 面板 URL 自身不得被当成上游地址
        assert!(!proxy_target_matches_source(&navigated, "https://v1.vossc.com"));
    }

    fn proxy_test_service(root: &std::path::Path) -> crate::service::book_service::BookService {
        crate::service::book_service::BookService::new(
            crate::crawler::http_client::HttpClient::new(20, None).unwrap(),
            crate::parser::rule_engine::RuleEngine::new().unwrap(),
            crate::storage::cache::file_cache::FileCache::new(root.join("cache")),
            root.to_str().unwrap(),
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn login_proxy_sends_single_headers_and_preserves_form_and_cookie() {
        use axum::{body::Bytes, http::{HeaderMap, StatusCode}, routing::get, Router};
        async fn page(headers: HeaderMap, body: Bytes) -> (StatusCode, HeaderMap, String) {
            let mut output = HeaderMap::new();
            for name in ["referer", "user-agent", "accept-language", "origin"] {
                if headers.get_all(name).iter().count() != 1 {
                    return (StatusCode::BAD_REQUEST, output, format!("duplicate {name}"));
                }
            }
            if body.is_empty() {
                output.insert("set-cookie", "login-session=fixture; Path=/".parse().unwrap());
            }
            output.insert("content-type", "application/json".parse().unwrap());
            (StatusCode::OK, output, serde_json::json!({
                "referer":headers["referer"].to_str().unwrap(),
                "ua":headers["user-agent"].to_str().unwrap(),
                "language":headers["accept-language"].to_str().unwrap(),
                "origin":headers["origin"].to_str().unwrap(),
                "cookie":headers.get("cookie").and_then(|v|v.to_str().ok()),
                "contentType":headers.get("content-type").and_then(|v|v.to_str().ok()),
                "body":String::from_utf8(body.to_vec()).unwrap(),
            }).to_string())
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/login.php", get(page).post(page))).await.unwrap();
        });
        let root = std::env::temp_dir().join(format!("reader-login-headers-{}", uuid::Uuid::new_v4()));
        let service = proxy_test_service(&root);
        let mut source = BookSource {
            book_source_url: base.clone(),
            header: Some(serde_json::json!({"Referer":format!("{base}/"), "User-Agent":"source-agent", "Accept-Language":"zh-CN"}).to_string()),
            ..Default::default()
        };
        let mut browser = HeaderMap::new();
        browser.insert("user-agent", "browser-agent".parse().unwrap());
        browser.insert("accept-language", "en-US".parse().unwrap());
        browser.insert("referer", "http://tauri.localhost/".parse().unwrap());
        browser.insert("origin", "http://reader.localhost".parse().unwrap());
        browser.insert("cookie", "unrelated=browser".parse().unwrap());
        let target = format!("{base}/login.php?jumpurl={}", urlencoding::encode(&format!("{base}/")));
        let first = forward_book_source_request(&service, &source, "fixture", &http::Method::GET, &browser, &target, None, &[]).await.unwrap();
        assert_eq!(first.status(), 200);
        let first: serde_json::Value = serde_json::from_slice(first.body()).unwrap();
        assert_eq!(first["referer"], format!("{base}/"));
        assert_eq!(first["ua"], "source-agent");
        assert_eq!(first["language"], "zh-CN");
        assert_eq!(first["origin"], base);
        assert!(first["cookie"].is_null());
        let mut headers: serde_json::Value = serde_json::from_str(source.header.as_ref().unwrap()).unwrap();
        headers["Content-Type"] = serde_json::json!("application/json");
        source.header = Some(headers.to_string());
        browser.insert("content-type", "application/x-www-form-urlencoded; charset=UTF-8".parse().unwrap());
        let body = b"username=fixture&password=local-test&act=login&usecookie=1";
        let second = forward_book_source_request(&service, &source, "fixture", &http::Method::POST, &browser, &target, Some(&target), body).await.unwrap();
        assert_eq!(second.status(), 200);
        let second: serde_json::Value = serde_json::from_slice(second.body()).unwrap();
        assert_eq!(second["referer"], format!("{base}/"));
        assert_eq!(second["contentType"], "application/x-www-form-urlencoded; charset=UTF-8");
        assert_eq!(second["body"], serde_json::Value::String(String::from_utf8_lossy(body).into_owned()));
        assert_eq!(second["cookie"], "login-session=fixture");
        server.abort();
    }

    #[tokio::test]
    #[ignore = "reads the supplied source and fetches its public login page using the production proxy"]
    async fn live_login_proxy_page() {
        let path = std::env::var("READER_LOGIN_SOURCE_FILE").expect("source JSON path");
        let value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let source = crate::model::book_source::book_source_from_value(value).unwrap();
        let root = std::env::temp_dir().join(format!("reader-live-login-{}", uuid::Uuid::new_v4()));
        let service = proxy_test_service(&root);
        let view = service.login_book_source(&source).await.unwrap();
        let target = view["url"].as_str().unwrap();
        let browser = http::HeaderMap::from_iter([
            (http::header::USER_AGENT, http::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0")),
            (http::header::ACCEPT_LANGUAGE, http::HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8")),
            (http::header::REFERER, http::HeaderValue::from_static("http://tauri.localhost/")),
            (http::HeaderName::from_static("sec-fetch-site"), http::HeaderValue::from_static("cross-site")),
            (http::HeaderName::from_static("sec-fetch-mode"), http::HeaderValue::from_static("navigate")),
            (http::HeaderName::from_static("sec-fetch-dest"), http::HeaderValue::from_static("iframe")),
        ]);
        let response = forward_book_source_request(&service, &source, "fixture-session", &http::Method::GET, &browser, target, None, &[]).await.unwrap();
        let status = response.status();
        let html = String::from_utf8(response.into_body()).unwrap();
        assert_eq!(status, 200, "{html}");
        assert!(html.contains("name=\"username\""), "login form must be present");
        assert!(html.contains("name=\"password\""));
        let output = std::path::PathBuf::from("target/login-proxy-fixtures");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("login.html"), &html).unwrap();
        let mut fixtures = vec![serde_json::json!({"url":target,"status":status.as_u16(),"file":"login.html","contentType":"text/html; charset=utf-8"})];
        let resources: Vec<String> = scraper::Html::parse_document(&html)
            .select(&scraper::Selector::parse("script[src],link[rel=stylesheet][href]").unwrap())
            .filter_map(|element| element.value().attr("src").or_else(|| element.value().attr("href")))
            .filter(|url| url.starts_with("/bookSourceProxy?"))
            .filter_map(|url| Url::parse(&format!("http://reader.localhost{url}")).ok())
            .filter_map(|url| url.query_pairs().find(|(name,_)|name=="url").map(|(_,value)|value.into_owned()))
            .collect();
        for (index, url) in resources.iter().enumerate() {
            let response = forward_book_source_request(&service, &source, "fixture-session", &http::Method::GET, &browser, url, Some(target), &[]).await.unwrap();
            assert_eq!(response.status(), 200, "resource {url}");
            let file = format!("resource-{index}");
            let content_type = response.headers().get(http::header::CONTENT_TYPE).unwrap().to_str().unwrap().to_string();
            std::fs::write(output.join(&file), response.body()).unwrap();
            fixtures.push(serde_json::json!({"url":url,"status":200,"file":file,"contentType":content_type}));
        }
        std::fs::write(output.join("report.json"), serde_json::to_vec_pretty(&serde_json::json!({"sourceName":source.book_source_name,"sourceUrl":source.book_source_url,"view":view,"responses":fixtures})).unwrap()).unwrap();
        eprintln!("production login proxy: HTTP 200, form present, {} same-site resources fetched", resources.len());
    }

    /// 业务接口地址**不能**被插进会话段。
    ///
    /// 打包产物里 `Pe("/site/info")` 会和 axios 的 baseURL(`/api`)再拼接。
    /// 若把这类字面量也改写成 `/__login/<s>/site/info`, 最终会请求
    /// `/api/__login/<s>/site/info` —— 上游根本没有这个地址, 表现为登录页
    /// 能挂载但数据全空。
    #[test]
    fn script_rewrite_leaves_business_paths_alone() {
        let script = r#"function dE(){return Pe("/site/info")} const Ab=e=>"/user/"+e;"#;
        let output = annotate_script_asset_prefix(script, "session-a");
        assert!(
            output.contains(r#"Pe("/site/info")"#),
            "业务接口地址必须原样保留: {output}"
        );
        assert!(
            output.contains(r#""/__login/session-a/user/""#),
            "资源基准要补会话段: {output}"
        );
    }

    /// HTML 里的绝对路径资源属性要补会话段, 相对/协议相对地址不动。
    #[test]
    fn html_resource_attrs_gain_login_session() {
        let html = r#"<script type="module" crossorigin src="/user/assets/index.js"></script>
<link rel="stylesheet" href='/user/assets/index.css'>
<img src="https://cdn.example.net/logo.png">
<script src="chunk.js"></script>"#;
        let output = rewrite_login_html_resource_attrs(html, "session-a");
        assert!(output.contains(r#"src="/__login/session-a/user/assets/index.js""#), "{output}");
        assert!(output.contains(r#"href='/__login/session-a/user/assets/index.css'"#), "{output}");
        // 跨域与相对地址保持原样。
        assert!(output.contains(r#"src="https://cdn.example.net/logo.png""#));
        assert!(output.contains(r#"src="chunk.js""#));
    }

    /// 打包产物里写死的分片前缀必须补上会话段。
    ///
    /// Vite 把资源基准烘进代码: `Ab=e=>"/user/"+e`。这个前缀不经过文档基准,
    /// 无论 iframe 挂在哪个路径下都会请求 `/user/assets/x.js` —— 那里没有会话,
    /// 后端无从判断该转发到哪, 只能 404。补上会话段后, 分片重新落回
    /// `/__login/<session>/user/assets/x.js`, 由登录代理转发。
    #[test]
    fn script_asset_prefix_gains_login_session() {
        for quote in ['"', '\''] {
            let script = format!(r#"const Ab=function(e){{return{quote}/user/{quote}+e}};"#);
            let output = annotate_script_asset_prefix(&script, "session-a");
            assert!(
                output.contains(&format!("{quote}/__login/session-a/user/{quote}")),
                "分片前缀应补会话段: {output}"
            );
        }
        // 根路径与协议相对地址不动。
        let untouched = "fetch('//cdn.example.com/x.js'); a('/');";
        assert_eq!(
            annotate_script_asset_prefix(untouched, "session-a"),
            untouched
        );
        // 空会话不做改写(退回旧行为, 由兜底路由处理)。
        let plain = r#"const Ab=function(e){return"/user/"+e};"#;
        assert_eq!(annotate_script_asset_prefix(plain, ""), plain);
    }

    /// 采集脚本**不得重新引入 URL 改写**。
    ///
    /// 登录页现在直接跑在上游原始路径上, 页面自己发出的地址天然正确。一旦
    /// 脚本把 fetch/XHR/资源地址改写成 `/bookSourceProxy?...`, 打包产物的
    /// 绝对路径(路由 base `/user/`、分片 `"/user/"+chunk`)就会全部 404,
    /// 单页应用永远挂载不起来 —— 这正是登录页长期白屏的根因。
    #[test]
    fn harvest_script_leaves_urls_untouched() {
        let script = build_cookie_harvest_script("https://m.example.site/book/1", "session-a");
        for forbidden in [
            "shouldProxy",
            "toProxy",
            "/bookSourceProxy",
            "XMLHttpRequest.prototype.open",
            "window.fetch =",
        ] {
            assert!(
                !script.contains(forbidden),
                "采集脚本不应再改写地址, 但出现了 {forbidden}: {}",
                script.chars().take(300).collect::<String>()
            );
        }
        // 仍然保留会话标识与上报能力。
        assert!(script.contains(r#"var loginSession = "session-a";"#));
        assert!(script.contains("reader:loginCookies"));
    }

    /// 书源 URL 无法解析时脚本同样不生成代理地址(也不应 panic)。
    #[test]
    fn harvest_script_handles_unparseable_source_domain() {
        let script = build_cookie_harvest_script("not a url", "session-b");
        assert!(script.contains(r#"var loginSession = "session-b";"#));
        assert!(!script.contains("/bookSourceProxy"));
    }

    /// HTML 改写只代理同域资源。
    ///
    /// 跨域资源一律保持原样, 由 WebView 自己去取 —— 见 `rewrite_source_panel_html`
    /// 的说明: 把它们一起收进代理会让每个子资源都变成一次宿主请求, 直连不通时
    /// 全部挂到 15s 超时, 足以把整个应用(乃至桌面)拖垮。这条用例固定住该边界。
    #[test]
    fn rewrite_source_panel_html_only_proxies_same_domain() {
        let html = r#"<html><head>
          <link rel="stylesheet" href="https://cdn.other.example/a.css">
          <script src="/static/app.js"></script>
          <a href="https://yckceo.com/detail/1">跨域详情</a>
          <a href="javascript:void(0)">脚本</a>
        </head></html>"#;
        let page_url = "https://yckceo.vip/";
        let allowed = "https://yckceo.vip/";

        let output = rewrite_source_panel_html(html, page_url, allowed, "panel-1");

        // 同域资源被代回代理。
        assert!(
            output.contains("__readerPanelUrl=https%3A%2F%2Fyckceo.vip%2Fstatic%2Fapp.js"),
            "同域资源应代回代理: {output}"
        );
        // 跨域资源保持原样, 不经过本进程。
        assert!(
            output.contains("https://cdn.other.example/a.css"),
            "跨域资源必须保持原样: {output}"
        );
        assert!(
            !output.contains("__readerPanelUrl=https%3A%2F%2Fcdn.other.example"),
            "跨域资源不能被收进代理"
        );
        assert!(output.contains("https://yckceo.com/detail/1"));
        // 非 http(s) 不改写。
        assert!(output.contains(r#"<a href="javascript:void(0)">"#));
    }

    /// 跨域二级页自己的**同域**资源仍要代理: 判定基准必须包含当前页面的域,
    /// 而不是只看订阅源最初的那个地址。
    #[test]
    fn rewrite_source_panel_uses_current_page_domain() {
        let html = r#"<a href="/detail/2">同域</a>"#;
        // 页面已经从 yckceo.vip 跳到 yckceo.com, 订阅源原始地址仍是 .vip。
        let output = rewrite_source_panel_html(
            html,
            "https://yckceo.com/",
            "https://yckceo.vip/",
            "panel-1",
        );

        assert!(
            output.contains("__readerPanelUrl=https%3A%2F%2Fyckceo.com%2Fdetail%2F2"),
            "应基于当前页面域改写: {output}"
        );
    }

    #[test]
    fn proxy_target_is_limited_to_source_domain_family() {
        assert!(proxy_target_matches_source(
            "https://passport.example.com/login",
            "https://www.example.com"
        ));
        assert!(proxy_target_matches_source(
            "https://www.example.co.uk/api",
            "https://m.example.co.uk"
        ));
        assert!(!proxy_target_matches_source(
            "https://evil.example.net/steal",
            "https://www.example.com"
        ));
        assert!(!proxy_target_matches_source(
            "https://127.0.0.1:8080/",
            "https://localhost:3000"
        ));
        assert!(!proxy_target_matches_source(
            "https://source.example:9000/",
            "https://source.example"
        ));
        assert!(!proxy_target_matches_source(
            "https://other.localhost/",
            "https://app.localhost"
        ));
    }

    // ─────────────────── 书源面板 (showBrowser) ───────────────────

    /// 二级链接统一留在面板内经代理导航, 不再交给系统浏览器。
    ///
    /// 背景: RSS 面板只托管**首屏** HTML; 页面里的链接指向上游站点。早先为
    /// 规避上游 `X-Frame-Options` 造成的白屏, 这些链接被交给系统浏览器。
    /// 但跳转实际走 `proxyUrl` —— 页面由后端抓取、重写后托管在 reader origin,
    /// 代理不透传那些限制头, 所以经代理渲染不会白屏。现在两种面板共用同一段
    /// click 兜底, 行为一致。
    #[test]
    fn panel_bridge_keeps_secondary_links_inside_panel() {
        let rss = build_source_panel_bridge_script("panel-rss", true);
        let book = build_source_panel_bridge_script("panel-book", false);

        for (label, script) in [("RSS", &rss), ("书源", &book)] {
            // 两种面板都不应再调用 window.open 把链接甩给系统浏览器。
            assert!(
                !script.contains(r#"window.open(url.href, "_blank", "noopener")"#),
                "{label} 面板不应再把二级链接交给系统浏览器"
            );
            assert!(
                !script.contains(r#"window.open(target.href, "_blank", "noopener")"#),
                "{label} 面板不应再用 window.open 外跳"
            );
            // 二级链接改走代理导航, 留在面板内。
            assert!(
                script.contains("window.location.assign(proxyUrl(url))"),
                "{label} 面板应把二级链接代回代理 URL"
            );
            // click 兜底挂在捕获阶段, 否则页面自己的 handler 可能先执行。
            assert!(script.contains("}, true);"));
            assert!(script.contains("event.button !== 0"));
        }

        // 标志仍按面板类型注入(保留兼容), 但不再影响外链处理。
        assert!(rss.contains("var externalLinks = true"));
        assert!(book.contains("var externalLinks = false"));

        // 书源面板的表单提交仍走代理(登录/注册依赖它)。
        assert!(book.contains("submitToProxy(event, target)"));
    }

    /// 源仓库的「一键导入」是 `legado://` / `yuedu://` 自定义协议, 系统浏览器
    /// 不认识, 交出去就是静默失败; `target="_blank"` 的二级链接若不拦, 会被
    /// 系统浏览器接管(表现为"点一下就跳出应用")。
    #[test]
    fn panel_bridge_handles_import_protocol_and_blank_links() {
        let book = build_source_panel_bridge_script("panel-book", false);

        // 两种面板都要识别导入协议并转给宿主, 由前端拉取并入库。
        assert!(
            book.contains("function importProtocolUrl(raw)"),
            "书源面板应能识别 legado:// / yuedu:// 导入链接"
        );
        assert!(
            book.contains("postToHost({ type: \"importProtocol\", url: href })"),
            "导入链接需要转给宿主处理"
        );
        assert!(
            book.contains(r#"/^(legado|yuedu):$/i"#),
            "只接管这两种导入协议"
        );

        // 两种面板都要处理导入协议(源仓库既可能从书源面板打开, 也可能从
        // RSS 面板打开), 且都要把 _blank 链接留在面板内经代理导航。
        let rss = build_source_panel_bridge_script("panel-rss", true);
        for (label, script) in [("RSS", &rss), ("书源", &book)] {
            assert!(
                script.contains("function importProtocolUrl(raw)"),
                "{label} 面板应能识别导入协议"
            );
            assert!(
                script.contains("window.location.assign(proxyUrl(url))"),
                "{label} 面板的 _blank 链接应留在面板内"
            );
        }
    }

    #[test]
    fn panel_bridge_script_exposes_legado_api_surface() {
        let script = build_source_panel_bridge_script("panel-1", false);

        // 插件面板依赖的宿主接口 (legado WebJsExtensions)
        for expected in [
            "window.java",
            "window.cache",
            "window.run",
            "ajax:",
            "showBrowser:",
            "getFromMemory:",
            "getFile:",
            "putFile:",
        ] {
            assert!(script.contains(expected), "桥缺少 {expected}");
        }
        // 同步 XHR(插件大量使用同步 ajax)与免预检 Content-Type
        assert!(script.contains("false); // false = 同步"));
        assert!(script.contains("text/plain"));
        // panelId 需转义进脚本
        assert!(script.contains("\"panel-1\""));
    }

    /// 插件用 `typeof java.createSymmetricCrypto !== 'undefined'` 探测原生加密能力;
    /// 探测失败会降级到「注入 CryptoJS」的回退路径(那条路径在桌面端会直接失败,
    /// 导致面板所有 API 调用无签名 → 书籍信息为空、段评加载不出来)。
    #[test]
    fn panel_bridge_script_declares_crypto_methods() {
        let script = build_source_panel_bridge_script("panel-1", false);
        for method in [
            "createSymmetricCrypto:",
            "aesEncodeToBase64String:",
            "aesEncodeToString:",
            "aesDecodeToString:",
            "desEncodeToBase64String:",
            "desEncodeToString:",
            "desDecodeToString:",
            "md5Encode:",
            "base64Encode:",
            "base64Decode:",
        ] {
            assert!(script.contains(method), "桥缺少加密方法 {method}");
        }
        // 需要能完成对外的 crypto 桥调用
        assert!(script.contains("bridgeSync(\"crypto\""));
    }

    #[test]
    fn panel_bridge_script_does_not_patch_xhr() {
        // 曾经 patch XMLHttpRequest.prototype 导致桥自身的 XHR 也被拦截(自递归)。
        let script = build_source_panel_bridge_script("panel-1", false);
        assert!(!script.contains("XMLHttpRequest.prototype"));
    }

    #[test]
    fn source_panel_page_injects_bridge_into_head() {
        let id = crate::service::book_service::register_source_panel(
            crate::service::book_service::SourcePanelEntry {
                url: "https://panel.example/panel".to_string(),
                html: "<html><head><title>面板</title></head><body>内容</body></html>".to_string(),
                book_source_url: "https://source.example".to_string(),
                book_url: "https://source.example/book/1".to_string(),
                chapter_url: "https://source.example/book/1/1".to_string(),
                book_json: "{}".to_string(),
                chapter_json: "{}".to_string(),
                ..Default::default()
            },
        );

        let page = build_source_panel_page(&id).expect("面板页应可构建");
        // 原内容保留
        assert!(page.contains("<title>面板</title>"));
        assert!(page.contains("内容"));
        // 桥注入到 </head> 之前
        let bridge_at = page.find("window.java").expect("桥应被注入");
        let head_end = page.find("</head>").expect("应有 </head>");
        assert!(bridge_at < head_end, "桥应注入在 </head> 之前");
        // 页面里带上自己的 panelId
        assert!(page.contains(&id));

        // 未登记的面板返回 None
        assert!(build_source_panel_page("no-such-panel").is_none());
    }

    #[test]
    fn panel_registry_keeps_newest_and_evicts_oldest() {
        let make = |url: &str| crate::service::book_service::SourcePanelEntry {
            url: url.to_string(),
            html: "<html></html>".to_string(),
            book_source_url: "https://evict.example".to_string(),
            book_url: String::new(),
            chapter_url: String::new(),
            book_json: "{}".to_string(),
            chapter_json: "{}".to_string(),
            ..Default::default()
        };

        // 登记超过上限, 最旧的应被淘汰, 最新的应仍在
        let mut ids = Vec::new();
        for index in 0..12 {
            ids.push(crate::service::book_service::register_source_panel(make(&format!(
                "https://evict.example/{index}"
            ))));
        }
        let newest = ids.last().unwrap();
        let oldest = ids.first().unwrap();
        assert!(
            crate::service::book_service::get_source_panel(newest).is_some(),
            "最新面板不应被淘汰"
        );
        assert!(
            crate::service::book_service::get_source_panel(oldest).is_none(),
            "最旧面板应被淘汰(按插入顺序, 而非 HashMap 随机序)"
        );
    }

    /// RSS 的 `singleUrl` 源(源地址本身就是"一篇文章")复用书源面板的**静态渲染**
    /// 路径: 后端先抓好 HTML 再登记面板, 因此不查书源表。
    ///
    /// 这条用例守住该复用点 —— 面板 HTML 必须带上桥脚本与 `<base>`, 否则面板里的
    /// 相对资源会以 `reader.localhost` 为基准解析而全部 404。
    ///
    /// 另外验证 `external_links` 从面板条目一路传到渲染出的桥脚本: 这正是
    /// "点二级链接白屏" 的修复开关(上游页面的 X-Frame-Options 会让 iframe 拒绝渲染)。
    #[test]
    fn rss_static_panel_renders_bridge_and_base_for_non_book_source() {
        let page_url = "https://panel.example.vip/";
        let html = "<html><head><title>面板页</title></head><body><a href=\"/a\">x</a></body></html>";
        // 关键: 面板归属留空 —— RSS 源没有书源记录, 走的正是静态渲染分支。
        // `external_links: true` 与 `open_rss_source_panel` 里的设置一致。
        let entry = crate::service::book_service::SourcePanelEntry {
            url: page_url.to_string(),
            html: html.to_string(),
            external_links: true,
            ..Default::default()
        };
        let panel_id = crate::service::book_service::register_source_panel(entry.clone());

        let rendered = render_source_panel_page(&panel_id, &entry, page_url, html);

        assert!(
            rendered.contains("__readerPanelBridgeReady") || rendered.contains("readerPanelBridge"),
            "面板必须注入桥脚本: {}",
            rendered.chars().take(200).collect::<String>()
        );
        assert!(
            rendered.contains("<base href"),
            "必须注入 <base> 让相对资源指回上游站点"
        );
        assert!(
            rendered.contains("panel.example.vip"),
            "面板 URL 应出现在页面里(桥用它做同源校验与代理拼接)"
        );
        // 原页面内容保留(静态渲染不是重写页面, 只是插入桥)。
        assert!(rendered.contains("面板页"), "原页面内容应保留");
        // 外链开关必须随条目进入渲染结果, 否则二级链接仍会在 iframe 内白屏。
        assert!(
            rendered.contains("var externalLinks = true"),
            "RSS 面板渲染结果必须开启外链跳转"
        );
        assert!(
            rendered.contains(r#"addEventListener("click""#),
            "必须带 click 兜底(老 WebView 无 navigation API)"
        );
    }
