//! 真实 MusicFree 插件冒烟测试（默认忽略）。
//!
//! 用于验证第三方插件在本地宿主里的实际可用性：加载脚本、调用 search，
//! 打印返回内容或错误原因。需要联网，因此不进入常规门禁。
//!
//! 运行方式：
//! ```powershell
//! $env:MF_PLUGIN_DIR = "$env:TEMP\mf-test"   # 存放 .js 插件的目录
//! cargo test --test mf_real_plugin -- --ignored --nocapture
//! ```
//!
//! 设置了 `MF_HTTP_PROXY` 时，插件网络请求走该代理。

use std::time::Duration;

use reader_rust::crawler::http_client::HttpClient;
use reader_rust::service::music::plugin::worker::PluginWorker;
use reader_rust::service::music::plugin::PluginRegistry;
use serde_json::{json, Value};
use sqlx::SqlitePool;

async fn call(worker: &PluginWorker, method: &str, args: Value) -> Result<String, String> {
    let receiver = worker.submit(method, &args).map_err(|err| err.to_string())?;
    receiver.await.map_err(|err| err.to_string())?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn real_plugin_search_smoke() {
    let dir = match std::env::var("MF_PLUGIN_DIR") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            println!("未设置 MF_PLUGIN_DIR，跳过");
            return;
        }
    };
    let proxy = std::env::var("MF_HTTP_PROXY").ok().filter(|v| !v.trim().is_empty());
    let http = HttpClient::new(25, proxy.clone()).expect("http client");

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read plugin dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "js"))
        .collect();
    files.sort();

    for path in files {
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                println!("[{name}] 读取失败: {err}");
                continue;
            }
        };
        let worker = PluginWorker::spawn(&name, &source, http.clone(), Default::default());
        let outcome = tokio::time::timeout(
            Duration::from_secs(60),
            call(&worker, "search", json!(["周杰伦", 1, "music"])),
        )
        .await;

        match outcome {
            Ok(Ok(raw)) => {
                let preview: String = raw.chars().take(300).collect();
                let parsed: Option<Value> = serde_json::from_str(&raw).ok();
                let items = parsed
                    .as_ref()
                    .and_then(|value| value.get("data"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                println!("[{name}] 搜索 OK count={} preview={preview}", items.len());

                // 再逐条取流：默认音质可能要求 VIP，换低音质与多首歌再判断。
                let mut succeeded = 0;
                for item in items.iter().take(4) {
                    let mut line = String::new();
                    for quality in ["128k", "standard", "low"] {
                        let media = tokio::time::timeout(
                            Duration::from_secs(45),
                            call(&worker, "getMediaSource", json!([item, quality])),
                        )
                        .await;
                        match media {
                            Ok(Ok(raw)) => {
                                let brief: String = raw.chars().take(140).collect();
                                line.push_str(&format!(" {quality}=OK {brief}"));
                                succeeded += 1;
                                break;
                            }
                            Ok(Err(err)) => line.push_str(&format!(" {quality}=ERR({err})")),
                            Err(_) => line.push_str(&format!(" {quality}=TIMEOUT")),
                        }
                    }
                    let title = item.get("title").and_then(Value::as_str).unwrap_or("?");
                    println!("[{name}] 取流《{title}》{line}");
                }
                println!("[{name}] 取流成功 {succeeded} 条");
            }
            Ok(Err(err)) => println!("[{name}] 插件错误: {err}"),
            Err(_) => println!("[{name}] 超时"),
        }
    }
}

/// 订阅清单整链：下载 plugins.json → 批量安装 → 搜索。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn real_subscription_install_smoke() {
    let url = std::env::var("MF_SUBSCRIBE_URL")
        .unwrap_or_else(|_| "https://hebijunge.github.io/musicfree-plugins/plugins.json".to_string());
    let proxy = std::env::var("MF_HTTP_PROXY").ok().filter(|v| !v.trim().is_empty());
    let http = HttpClient::new(30, proxy).expect("http client");

    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("memory db");
    sqlx::migrate!("src/storage/db/migrations")
        .run(&pool)
        .await
        .expect("migrations");
    let dir = std::env::temp_dir().join(format!("mf-sub-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let registry = PluginRegistry::new(pool, dir.to_str().expect("utf8 path"), http.clone());

    let results = registry.install_from_urls(&url).await.expect("install");
    let mut ok = 0;
    println!("==== 安装明细 ====");
    for (index, item) in results.iter().enumerate() {
        if item.success {
            ok += 1;
        }
        let label = if item.success { "成功" } else { "失败" };
        println!(
            "{:>2}. [{label}] {} ({}) {}",
            index + 1,
            if item.name.is_empty() { &item.url } else { &item.name },
            item.platform,
            item.error
        );
    }
    println!("安装汇总: 清单地址 {} 条, 成功 {ok} 条", results.len());
    assert_eq!(ok, results.len(), "有插件安装失败");

    let plugins = registry.list().await.expect("list");
    println!("==== 加载检查 ==== 入库 {} 个", plugins.len());

    let mut loaded = 0;
    let mut failures: Vec<(String, String)> = Vec::new();
    for plugin in &plugins {
        let label = if plugin.name.is_empty() {
            plugin.platform.clone()
        } else {
            plugin.name.clone()
        };
        let source = match std::fs::read_to_string(&plugin.file_path) {
            Ok(source) => source,
            Err(err) => {
                failures.push((label, format!("文件不可读: {err}")));
                continue;
            }
        };
        let worker = PluginWorker::spawn(&plugin.platform, &source, http.clone(), Default::default());
        // 只读取插件导出的元信息，不发网络请求：验证脚本能在宿主垫片里成功挂载。
        match tokio::time::timeout(
            Duration::from_secs(30),
            call(&worker, "__mf_user_variables", json!([])),
        )
        .await
        {
            Ok(Ok(_)) => loaded += 1,
            Ok(Err(err)) => failures.push((label, err)),
            Err(_) => failures.push((label, "加载超时".to_string())),
        }
    }
    println!("加载汇总: 成功 {loaded} / {}", plugins.len());
    for (name, err) in &failures {
        println!("  加载失败 {name}: {err}");
    }
    // 这里只做诊断输出：部分插件依赖尚未实现的库（如 cheerio），
    // 不阻塞门禁，失败项会逐条列出便于后续补齐。

    for plugin in plugins.iter().take(3) {
        let source = std::fs::read_to_string(&plugin.file_path).expect("read plugin");
        let worker = PluginWorker::spawn(&plugin.platform, &source, http.clone(), Default::default());
        match tokio::time::timeout(
            Duration::from_secs(60),
            call(&worker, "search", json!(["周杰伦", 1, "music"])),
        )
        .await
        {
            Ok(Ok(raw)) => println!("[{}] 搜索 OK {}", plugin.name, &raw.chars().take(160).collect::<String>()),
            Ok(Err(err)) => println!("[{}] 搜索失败: {err}", plugin.name),
            Err(_) => println!("[{}] 搜索超时", plugin.name),
        }
    }

    let _ = std::fs::remove_dir_all(dir);
}
