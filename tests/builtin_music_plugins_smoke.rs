//! 内置音源可用性冒烟测试。
//!
//! 遍历 `desktop/src-tauri/builtin-music-plugins/*.js`，逐个加载插件并调用
//! `search`，用于发布前确认随包分发的音源至少有主流平台能返回结果。
//!
//! 该测试会访问真实站点（部分站点的可用性随时间变化），因此默认 `#[ignore]`，
//! 发布前手动执行：
//!
//! ```text
//! cargo test --test builtin_music_plugins_smoke -- --ignored --nocapture
//! ```

use reader_rust::crawler::http_client::HttpClient;
use reader_rust::service::music::plugin::worker::PluginWorker;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 最少可用音源数：主流平台（QQ/网易云/酷狗/酷我/咪咕/汽水/B站/番茄）应当在场。
const MIN_AVAILABLE: usize = 5;

fn plugin_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("desktop")
        .join("src-tauri")
        .join("builtin-music-plugins")
}

/// 单个插件的搜索结果条数；`Err` 表示加载或调用失败。
fn search_count(file: &Path) -> Result<usize, String> {
    let source = std::fs::read_to_string(file).map_err(|err| format!("读取失败: {err}"))?;
    let http = HttpClient::new(10, None).map_err(|err| format!("HTTP 客户端创建失败: {err}"))?;
    let label = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown");
    let worker = PluginWorker::spawn(label, &source, http, HashMap::new());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("运行时创建失败: {err}"))?;
    runtime.block_on(async {
        let receiver = worker
            .submit("search", &json!(["周杰伦", 1, "music"]))
            .map_err(|err| format!("提交失败: {err}"))?;
        let raw = tokio::time::timeout(Duration::from_secs(15), receiver)
            .await
            .map_err(|_| "搜索超时".to_string())?
            .map_err(|_| "通道关闭".to_string())?
            .map_err(|err| format!("插件报错: {err}"))?;

        let value: Value = serde_json::from_str(&raw).map_err(|err| format!("返回非 JSON: {err}"))?;
        let count = value
            .get("data")
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or(0);
        Ok(count)
    })
}

#[test]
#[ignore = "会访问真实音源站点, 仅在发布前手动执行"]
fn bundled_plugins_return_search_results() {
    let dir = plugin_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("读取{}失败: {err}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("js"))
        .collect();
    files.sort();

    assert!(!files.is_empty(), "内置音源目录为空: {}", dir.display());

    let mut available: Vec<(String, usize)> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();

    for file in &files {
        let label = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        match search_count(file) {
            Ok(count) if count > 0 => {
                println!("[OK]   {label}: {count} 条");
                available.push((label, count));
            }
            Ok(_) => {
                println!("[EMPTY] {label}: 0 条");
                failed.push((label, "搜索结果为空".to_string()));
            }
            Err(err) => {
                println!("[FAIL] {label}: {err}");
                failed.push((label, err));
            }
        }
    }

    println!(
        "\n可用 {}/{}，失败 {}",
        available.len(),
        files.len(),
        failed.len()
    );
    assert!(
        available.len() >= MIN_AVAILABLE,
        "可用音源仅 {} 个，低于下限 {MIN_AVAILABLE}",
        available.len()
    );
}
