//! 音乐插件运行时集成测试：用本地 axum 上游验证 fetch / XMLHttpRequest 桥。
//!
//! 不访问外网, 因此可以作为常规门禁的一部分运行。

use axum::{response::Json, routing::get, Router};
use reader_rust::crawler::http_client::HttpClient;
use reader_rust::service::music::plugin::worker::PluginWorker;
use serde_json::{json, Value};
use std::time::Duration;

async fn call(worker: &PluginWorker, method: &str, args: Value) -> Value {
    let raw = tokio::time::timeout(Duration::from_secs(8), async {
        worker
            .submit(method, &args)
            .expect("submit")
            .await
            .expect("reply")
            .expect("plugin result")
    })
    .await
    .expect("plugin call timeout");
    serde_json::from_str(&raw).expect("json result")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direct_http_from_separate_thread_reaches_local_upstream() {
    let app = Router::new().route("/ping", get(|| async { Json(json!({ "ok": true })) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let result = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let http = HttpClient::new(5, None).unwrap();
        runtime.block_on(async {
            let client = http.client();
            client.get(format!("{base}/ping")).send().await.map(|r| r.status().as_u16())
        })
    })
    .await
    .unwrap();

    assert_eq!(result.unwrap(), 200);
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plugin_can_fetch_and_use_xhr_against_local_upstream() {
    let app = Router::new()
        .route(
            "/api",
            get(|| async { Json(json!({ "data": [{ "id": "7", "title": "网络歌曲", "artist": "A" }] })) }),
        )
        .route(
            "/lyric",
            get(|| async { Json(json!({ "lrc": "[00:01.00]第一句" })) }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let source = format!(
        r#"
module.exports = {{
  platform: "fetch-fixture",
  version: "1.0.0",
  search: async function (query, page, type) {{
    var res = await fetch("{base}/api");
    var payload = await res.json();
    return {{ isEnd: true, data: payload.data }};
  }},
  getLyric: function (item) {{
    return new Promise(function (resolve, reject) {{
      var xhr = new XMLHttpRequest();
      xhr.open('GET', "{base}/lyric");
      xhr.onload = function () {{ resolve(JSON.parse(xhr.responseText).lrc); }};
      xhr.onerror = function () {{ reject(new Error('xhr failed')); }};
      xhr.send();
    }});
  }}
}};
"#
    );

    let http = HttpClient::new(5, None).unwrap();
    let worker = PluginWorker::spawn(
        "fetch-fixture",
        &source,
        http,
        std::collections::HashMap::new(),
    );

    let search = call(&worker, "search", json!(["q", 1, "music"])).await;
    assert_eq!(search["isEnd"], json!(true));
    assert_eq!(search["data"][0]["title"], json!("网络歌曲"));

    let lyric = call(&worker, "getLyric", json!([{ "platform": "fetch-fixture", "id": "7" }])).await;
    assert_eq!(lyric, json!("[00:01.00]第一句"));

    server.abort();
}
