//! 一次性维护工具(手动运行): 按与 App 相同的搜索可用性检测扫描书源, 清理失效书源。
//!
//! 这是外部命令行工具而非 CI 测试, 走 `#[ignore]`, 需要显式提供数据目录:
//!
//! ```powershell
//! $env:READER_CLEANUP_DATA_DIR = 'C:\path\to\data'
//! $env:READER_CLEANUP_LIMIT = '0'        # 0 = 全部, >0 = 只测前 N 个
//! $env:READER_CLEANUP_DELETE = '1'       # 1 = 落库删除; 0 = 只标记失效分组
//! $env:READER_CLEANUP_CONCURRENCY = '16'
//! cargo test --test source_cleanup_live -- --ignored --nocapture
//! ```
//!
//! 规则: 没有 searchUrl/ruleSearch 的探索型书源无法用搜索判断可用性, 一律保留;
//! 有搜索规则的源连续失败两次才判定失效, 避免网络抖动造成误删。

use futures::StreamExt;
use reader_rust::app::bootstrap::build_state;
use reader_rust::app::config::AppConfig;
use reader_rust::service::book_source_service::set_invalid_book_source_group;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "访问真实站点, 手动运行"]
async fn clean_unusable_sources() {
    let data_dir = std::env::var("READER_CLEANUP_DATA_DIR")
        .expect("需要 READER_CLEANUP_DATA_DIR(包含 reader.db 的 data 目录)");
    let limit: usize = std::env::var("READER_CLEANUP_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let delete = std::env::var("READER_CLEANUP_DELETE").map(|v| v == "1").unwrap_or(false);
    // 只统计不落库(试跑)时用 READER_CLEANUP_MARK=0。
    let mark = std::env::var("READER_CLEANUP_MARK").map(|v| v == "1").unwrap_or(delete);
    let concurrency: usize = std::env::var("READER_CLEANUP_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(16)
        .clamp(1, 32);

    let cfg = AppConfig {
        database_url: std::path::Path::new(&data_dir).join("reader.db").to_string_lossy().into_owned(),
        storage_dir: std::path::Path::new(&data_dir).join("storage").to_string_lossy().into_owned(),
        assets_dir: std::path::Path::new(&data_dir)
            .join("storage")
            .join("assets")
            .to_string_lossy()
            .into_owned(),
        request_timeout_secs: 8,
        ..AppConfig::default()
    };
    let state = build_state(&cfg).await.expect("build state");

    let mut sources = state.book_source_service.list("default").await.expect("list sources");
    // 书架(含换源候选)引用的书源一律保留: 删掉会让现有书直接无法打开/无法换源。
    let books = state.book_service.get_bookshelf("default").await.unwrap_or_default();
    let mut protected = std::collections::HashSet::new();
    for book in &books {
        if !book.origin.trim().is_empty() {
            protected.insert(book.origin.trim().to_string());
        }
        for candidate in book.source_candidates.iter().flatten() {
            if !candidate.origin.trim().is_empty() {
                protected.insert(candidate.origin.trim().to_string());
            }
        }
    }
    eprintln!("保护 {} 个被书架引用的书源", protected.len());
    sources.retain(|source| !protected.contains(source.book_source_url.trim()));
    if limit > 0 {
        sources.truncate(limit);
    }
    let total = sources.len();
    eprintln!("开始检测 {total} 个书源, 并发 {concurrency}");

    let mut stream = futures::stream::iter(sources.into_iter().map(|source| {
        let service = state.book_service.clone();
        async move {
            let has_search = source
                .search_url
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                && source.rule_search.is_some();
            if !has_search {
                return (source, true, false);
            }
            let first = service
                .test_book_source_availability("default", &source, None)
                .await;
            if first.valid {
                return (source, true, true);
            }
            let second = service
                .test_book_source_availability("default", &source, None)
                .await;
            (source, second.valid, true)
        }
    }))
    .buffer_unordered(concurrency);

    let mut invalid = Vec::new();
    let mut tested = 0usize;
    while let Some((mut source, valid, checked)) = stream.next().await {
        if checked {
            tested += 1;
        }
        if checked && !valid {
            eprintln!("失效: {} [{}]", source.book_source_url, source.book_source_name);
            if !mark {
                continue;
            }
            set_invalid_book_source_group(&mut source, true);
            if let Err(err) = state.book_source_service.save("default", source.clone()).await {
                eprintln!("标记失效失败 {}: {err:?}", source.book_source_url);
                continue;
            }
            invalid.push(source.book_source_url);
        }
    }

    if delete && !invalid.is_empty() {
        match state.book_source_service.delete_many("default", &invalid).await {
            Ok(count) => eprintln!("已删除 {count} 个失效书源"),
            Err(err) => eprintln!("删除失败: {err:?}"),
        }
    }

    eprintln!(
        "完成: 检测 {tested}, 失效 {}, 删除 {}",
        invalid.len(),
        if delete { invalid.len() } else { 0 }
    );
}
