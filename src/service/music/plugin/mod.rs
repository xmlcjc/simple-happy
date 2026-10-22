//! 音乐插件注册表：负责插件的落盘、元信息持久化与版本管理。
//!
//! 插件脚本放在 `{storage_dir}/music-plugins/`，数据库只保存元信息与文件路径。
//! 与 MusicFree 一致：以 `platform` 作为插件唯一标识，同名插件按版本号覆盖。
//!
//! 运行时的 JS 执行在 `worker.rs`（P1 接入），本文件保持纯数据层，
//! 方便单测与后续替换。

pub mod manifest;
pub mod shim;
pub mod worker;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::{Row, SqlitePool};

use crate::crawler::fetcher::{self, HttpMethod, RequestSpec};
use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;
use crate::service::music::model::{
    MusicPlugin, PluginInstallResult, PluginSubscription, PluginVariable,
};

use self::manifest::parse_manifest;
use self::worker::PluginWorker;

#[derive(Clone)]
pub struct PluginRegistry {
    pool: SqlitePool,
    dir: PathBuf,
    http: HttpClient,
    /// 插件名 -> 正在运行的 JS 线程。懒加载: 第一次调用时才创建。
    workers: Arc<Mutex<HashMap<String, Arc<PluginWorker>>>>,
}

impl PluginRegistry {
    pub fn new(pool: SqlitePool, storage_dir: &str, http: HttpClient) -> Self {
        let dir = Path::new(storage_dir).join("music-plugins");
        if let Err(err) = std::fs::create_dir_all(&dir) {
            tracing::warn!("创建音乐插件目录失败: {err}");
        }
        Self {
            pool,
            dir,
            http,
            workers: Arc::default(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub async fn list(&self) -> Result<Vec<MusicPlugin>, AppError> {
        let rows = sqlx::query(
            "SELECT platform, name, version, author, src_url, file_path, enabled, description, created_at, updated_at
             FROM music_plugin ORDER BY sort_order ASC, created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_plugin).collect())
    }

    pub async fn get(&self, platform: &str) -> Result<Option<MusicPlugin>, AppError> {
        let row = sqlx::query(
            "SELECT platform, name, version, author, src_url, file_path, enabled, description, created_at, updated_at
             FROM music_plugin WHERE platform = ?1",
        )
        .bind(platform)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_plugin))
    }

    /// 调用插件导出函数。`args` 是位置参数数组(JSON), 返回值已解包为 JSON。
    ///
    /// 超时或线程退出会丢弃该插件的工作线程, 下次调用重新加载, 避免一个坏
    /// 插件一直占用资源。
    pub async fn call(
        &self,
        plugin: &MusicPlugin,
        method: &str,
        args: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, AppError> {
        let variables = self.variable_values(&plugin.platform).await?;
        let worker = self.worker_for(plugin, &variables)?;
        let receiver = worker.submit(method, &args)?;
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(Ok(raw))) => serde_json::from_str(&raw).map_err(|e| {
                AppError::BadRequest(format!("插件返回了无法解析的数据: {e}"))
            }),
            Ok(Ok(Err(message))) => Err(AppError::BadRequest(message)),
            Ok(Err(_)) => {
                self.drop_worker(&plugin.platform);
                Err(AppError::BadRequest(format!(
                    "插件 {} 的线程已退出",
                    plugin.name
                )))
            }
            Err(_) => {
                self.drop_worker(&plugin.platform);
                Err(AppError::BadRequest(format!(
                    "插件 {} 响应超时，已重置该插件的运行时",
                    plugin.name
                )))
            }
        }
    }

    /// 懒加载插件线程。插件文件缺失或脚本无法加载时返回明确错误。
    fn worker_for(
        &self,
        plugin: &MusicPlugin,
        variables: &HashMap<String, String>,
    ) -> Result<Arc<PluginWorker>, AppError> {
        let mut workers = self
            .workers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(worker) = workers.get(&plugin.platform) {
            return Ok(worker.clone());
        }
        let source = std::fs::read_to_string(&plugin.file_path).map_err(|e| {
            AppError::BadRequest(format!("插件文件不可读，请重新安装（{e}）"))
        })?;
        let worker = Arc::new(PluginWorker::spawn(
            &plugin.platform,
            &source,
            self.http.clone(),
            variables.clone(),
        ));
        workers.insert(plugin.platform.clone(), worker.clone());
        Ok(worker)
    }

    /// 丢弃插件线程(卸载、停用、升级或超时后调用)。
    pub fn drop_worker(&self, platform: &str) {
        let mut workers = self
            .workers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        workers.remove(platform);
    }

    /// 安装或升级插件。返回落库后的元信息。
    pub async fn install(
        &self,
        source: &str,
        original_name: Option<&str>,
    ) -> Result<MusicPlugin, AppError> {
        self.install_with_fallback(source, original_name, None).await
    }

    /// `fallback_src_url` 用于按 URL 安装：插件本身没写 `srcUrl` 时用下载地址补齐，
    /// 否则装完就永远没有可更新的源。
    async fn install_with_fallback(
        &self,
        source: &str,
        original_name: Option<&str>,
        fallback_src_url: Option<&str>,
    ) -> Result<MusicPlugin, AppError> {
        let mut manifest = parse_manifest(source)?;
        if manifest.src_url.is_empty() {
            manifest.src_url = fallback_src_url.unwrap_or("").trim().to_string();
        }
        let existing = self.get(&manifest.platform).await?;
        if let Some(current) = &existing {
            // 与 MusicFree 一致: 只拒绝降级, 同版本允许覆盖安装(便于修复损坏的脚本)。
            if version_cmp(&manifest.version, &current.version) < 0 {
                return Err(AppError::BadRequest(format!(
                    "已安装更新版本的插件（{} {}）",
                    current.platform, current.version
                )));
            }
            // 本地重装同版本时插件源码可能没写 srcUrl, 不应丢掉原有的更新地址。
            if manifest.src_url.is_empty() {
                manifest.src_url = current.src_url.clone();
            }
        }

        let file_name = format!(
            "{}-{}.js",
            sanitize_file_stem(&manifest.platform),
            uuid::Uuid::new_v4()
        );
        let file_path = self.dir.join(&file_name);
        std::fs::write(&file_path, source)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("写入插件文件失败: {e}")))?;

        // 升级时清掉旧脚本, 避免目录无限增长。
        if let Some(current) = &existing {
            if !current.file_path.is_empty() {
                let _ = std::fs::remove_file(&current.file_path);
            }
        }

        let display_name = if manifest.name.is_empty() {
            original_name
                .map(|name| name.trim_end_matches(".js").to_string())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| manifest.platform.clone())
        } else {
            manifest.name.clone()
        };
        let now = chrono::Utc::now().timestamp_millis();
        let created_at = existing.as_ref().map(|item| item.created_at).unwrap_or(now);
        let path_text = file_path.to_string_lossy().to_string();

        sqlx::query(
            "INSERT INTO music_plugin
               (platform, name, version, author, src_url, file_path, enabled, description,
                sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7,
               COALESCE((SELECT MAX(sort_order) + 1 FROM music_plugin), 0), ?8, ?9)
             ON CONFLICT(platform) DO UPDATE SET
               name = excluded.name,
               version = excluded.version,
               author = excluded.author,
               src_url = excluded.src_url,
               file_path = excluded.file_path,
               enabled = 1,
               description = excluded.description,
               updated_at = excluded.updated_at",
        )
        .bind(&manifest.platform)
        .bind(&display_name)
        .bind(&manifest.version)
        .bind(&manifest.author)
        .bind(&manifest.src_url)
        .bind(&path_text)
        .bind(&manifest.description)
        .bind(created_at)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // 升级后旧脚本的线程必须重建, 否则仍在执行上一版代码。
        self.drop_worker(&manifest.platform);

        Ok(MusicPlugin {
            platform: manifest.platform,
            name: display_name,
            version: manifest.version,
            author: manifest.author,
            src_url: manifest.src_url,
            file_path: path_text,
            enabled: true,
            description: manifest.description,
            created_at,
            updated_at: now,
        })
    }

    /// 卸载插件并删除脚本文件。返回是否真的删掉了记录。
    pub async fn uninstall(&self, platform: &str) -> Result<bool, AppError> {
        self.drop_worker(platform);
        if let Some(item) = self.get(platform).await? {
            if !item.file_path.is_empty() {
                let _ = std::fs::remove_file(&item.file_path);
            }
        }
        let result = sqlx::query("DELETE FROM music_plugin WHERE platform = ?1")
            .bind(platform)
            .execute(&self.pool)
            .await?;
        let _ = sqlx::query("DELETE FROM music_plugin_variable WHERE platform = ?1")
            .bind(platform)
            .execute(&self.pool)
            .await;
        Ok(result.rows_affected() > 0)
    }

    /// 读取插件已保存的用户变量。
    pub async fn variable_values(
        &self,
        platform: &str,
    ) -> Result<HashMap<String, String>, AppError> {
        let rows = sqlx::query(
            "SELECT key, value FROM music_plugin_variable WHERE platform = ?1",
        )
        .bind(platform)
        .fetch_all(&self.pool)
        .await?;
        let mut map = HashMap::new();
        for row in rows {
            map.insert(row.get::<String, _>("key"), row.get::<String, _>("value"));
        }
        Ok(map)
    }

    /// 整体覆盖保存用户变量；保存后必须重建插件线程，让新值生效。
    pub async fn set_variables(
        &self,
        platform: &str,
        values: &HashMap<String, String>,
    ) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM music_plugin_variable WHERE platform = ?1")
            .bind(platform)
            .execute(&mut *tx)
            .await?;
        for (key, value) in values {
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            sqlx::query(
                "INSERT INTO music_plugin_variable (platform, key, value) VALUES (?1, ?2, ?3)",
            )
            .bind(platform)
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.drop_worker(platform);
        Ok(())
    }

    /// 读取插件声明的变量定义，供设置界面渲染表单。
    pub async fn variable_definitions(
        &self,
        plugin: &MusicPlugin,
    ) -> Result<Vec<PluginVariable>, AppError> {
        let value = self
            .call(
                plugin,
                "__mf_user_variables",
                serde_json::json!([]),
                Duration::from_secs(10),
            )
            .await?;
        let list = value.as_array().cloned().unwrap_or_default();
        Ok(list
            .into_iter()
            .filter_map(|raw| {
                let key = raw
                    .get("key")
                    .and_then(|item| item.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if key.is_empty() {
                    return None;
                }
                Some(PluginVariable {
                    key,
                    name: raw
                        .get("name")
                        .and_then(|item| item.as_str())
                        .unwrap_or("")
                        .to_string(),
                    hint: raw
                        .get("hint")
                        .and_then(|item| item.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
            })
            .collect())
    }

    /// 从插件声明的 srcUrl 拉取新版本并覆盖安装。
    pub async fn update(&self, platform: &str) -> Result<MusicPlugin, AppError> {
        let current = self
            .get(platform)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("插件 {platform} 未安装")))?;
        let src_url = current.src_url.trim();
        if src_url.is_empty() {
            return Err(AppError::BadRequest(
                "该插件没有提供更新地址（srcUrl）".to_string(),
            ));
        }
        let spec = RequestSpec {
            url: src_url.to_string(),
            method: HttpMethod::GET,
            retry: 1,
            timeout: Some(Duration::from_secs(30)),
            ..Default::default()
        };
        let response = fetcher::fetch(&self.http, spec)
            .await
            .map_err(|e| AppError::BadRequest(format!("下载插件失败: {e}")))?;
        if !(200..300).contains(&response.status) {
            return Err(AppError::BadRequest(format!(
                "下载插件失败（HTTP {}）",
                response.status
            )));
        }
        let source = response.body;
        let mut manifest = parse_manifest(&source)?;
        // 防止 srcUrl 指向别的插件，覆盖掉当前插件。
        if manifest.platform != platform {
            return Err(AppError::BadRequest(format!(
                "更新文件属于插件 {}，与当前插件不一致，已拒绝覆盖",
                manifest.platform
            )));
        }
        // 更新文件没写 srcUrl 时沿用旧地址，避免下次无法更新。
        if manifest.src_url.is_empty() {
            manifest.src_url = current.src_url.clone();
        }
        std::fs::write(&current.file_path, &source)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("写入插件文件失败: {e}")))?;
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE music_plugin SET name = ?2, version = ?3, author = ?4, src_url = ?5,
               description = ?6, updated_at = ?7
             WHERE platform = ?1",
        )
        .bind(platform)
        .bind(&manifest.name)
        .bind(&manifest.version)
        .bind(&manifest.author)
        .bind(&manifest.src_url)
        .bind(&manifest.description)
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.drop_worker(platform);
        self.get(platform)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("插件 {platform} 未安装")))
    }

    /// 更新全部声明了 srcUrl 的插件；返回 (成功数, 失败列表)。
    pub async fn update_all(&self) -> Result<(usize, Vec<(String, String)>), AppError> {
        let mut updated = 0;
        let mut failures = Vec::new();
        for plugin in self.list().await? {
            if plugin.src_url.trim().is_empty() {
                continue;
            }
            match self.update(&plugin.platform).await {
                Ok(_) => updated += 1,
                Err(err) => failures.push((plugin.platform, err.to_string())),
            }
        }
        Ok((updated, failures))
    }

    /// 卸载全部插件（保留订阅源，与 MusicFree 一致）。
    pub async fn uninstall_all(&self) -> Result<u64, AppError> {
        let plugins = self.list().await?;
        for plugin in &plugins {
            self.drop_worker(&plugin.platform);
            if !plugin.file_path.is_empty() {
                let _ = std::fs::remove_file(&plugin.file_path);
            }
        }
        let result = sqlx::query("DELETE FROM music_plugin")
            .execute(&self.pool)
            .await?;
        let _ = sqlx::query("DELETE FROM music_plugin_variable")
            .execute(&self.pool)
            .await;
        Ok(result.rows_affected())
    }

    /// 按传入顺序重写 `sort_order`（未列出的插件排在其后）。
    pub async fn reorder(&self, platforms: &[String]) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        for (index, platform) in platforms.iter().enumerate() {
            sqlx::query("UPDATE music_plugin SET sort_order = ?2 WHERE platform = ?1")
                .bind(platform)
                .bind(index as i64)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// 按 URL 安装。输入可以是单个 `.js` 直链，也可以是 `.json` 订阅清单。
    ///
    /// 与 MusicFree 一致：失败不中断后续地址，逐条返回结果，由前端汇总展示。
    pub async fn install_from_urls(
        &self,
        input: &str,
    ) -> Result<Vec<PluginInstallResult>, AppError> {
        let urls = self.resolve_plugin_urls(input).await?;
        let mut results = Vec::with_capacity(urls.len());
        for url in urls {
            results.push(self.install_from_url(&url).await);
        }
        Ok(results)
    }

    /// 安装单个地址；失败写进结果而不是抛错。
    pub async fn install_from_url(&self, url: &str) -> PluginInstallResult {
        let mut result = PluginInstallResult {
            url: url.to_string(),
            ..Default::default()
        };
        match self.try_install_from_url(url).await {
            Ok(plugin) => {
                result.success = true;
                result.platform = plugin.platform;
                result.name = plugin.name;
                result.version = plugin.version;
            }
            Err(err) => result.error = err.to_string(),
        }
        result
    }

    async fn try_install_from_url(&self, url: &str) -> Result<MusicPlugin, AppError> {
        let source = self.fetch_plugin_source(url).await?;
        self.install_with_fallback(&source, None, Some(url)).await
    }

    /// 把输入解析成待安装地址列表：`.json` 订阅清单展开成多个 URL。
    async fn resolve_plugin_urls(&self, input: &str) -> Result<Vec<String>, AppError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AppError::BadRequest("请输入插件地址".to_string()));
        }
        let lower = input.to_ascii_lowercase();
        if !(lower.ends_with(".json") || lower.contains(".json?")) {
            return Ok(vec![input.to_string()]);
        }

        let body = self.fetch_plugin_source(input).await?;
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|_| AppError::BadRequest("订阅清单不是有效的 JSON".to_string()))?;
        let list = value
            .get("plugins")
            .and_then(serde_json::Value::as_array)
            .or_else(|| value.as_array())
            .or_else(|| value.get("urls").and_then(serde_json::Value::as_array))
            .ok_or_else(|| {
                AppError::BadRequest("订阅清单格式无效，应为 {plugins:[{url}]}".to_string())
            })?;

        let mut urls: Vec<String> = Vec::new();
        for item in list {
            let url = match item {
                serde_json::Value::String(text) => text.trim().to_string(),
                other => other
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            };
            if !url.is_empty() && !urls.contains(&url) {
                urls.push(url);
            }
        }
        if urls.is_empty() {
            return Err(AppError::BadRequest(
                "订阅清单里没有可用的插件地址".to_string(),
            ));
        }
        Ok(urls)
    }

    /// 下载插件脚本。显式禁用缓存，避免拿到 CDN 上的旧版本。
    async fn fetch_plugin_source(&self, url: &str) -> Result<String, AppError> {
        let spec = RequestSpec {
            url: url.to_string(),
            method: HttpMethod::GET,
            retry: 1,
            timeout: Some(Duration::from_secs(30)),
            headers: vec![
                ("Cache-Control".to_string(), "no-cache".to_string()),
                ("Pragma".to_string(), "no-cache".to_string()),
            ],
            ..Default::default()
        };
        let response = fetcher::fetch(&self.http, spec)
            .await
            .map_err(|e| AppError::BadRequest(format!("下载插件失败: {e}")))?;
        if response.status == 404 {
            return Err(AppError::BadRequest(
                "插件不存在，请联系插件作者".to_string(),
            ));
        }
        if !(200..300).contains(&response.status) {
            return Err(AppError::BadRequest(format!(
                "下载插件失败（HTTP {}）",
                response.status
            )));
        }
        Ok(response.body)
    }

    /// 订阅源列表（按添加顺序）。
    pub async fn subscriptions(&self) -> Result<Vec<PluginSubscription>, AppError> {
        let rows = sqlx::query(
            "SELECT id, name, url, created_at FROM music_plugin_subscription
             WHERE user_ns = 'default' ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| PluginSubscription {
                id: row.get("id"),
                name: row.get("name"),
                url: row.get("url"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// 新增订阅源。地址必须是 `.js` 或 `.json`（与 MusicFree 的订阅校验一致）。
    pub async fn add_subscription(
        &self,
        name: &str,
        url: &str,
    ) -> Result<PluginSubscription, AppError> {
        let url = url.trim();
        if url.is_empty() {
            return Err(AppError::BadRequest("请输入订阅地址".to_string()));
        }
        let lower = url.to_ascii_lowercase();
        if !(lower.ends_with(".js") || lower.ends_with(".json")) {
            return Err(AppError::BadRequest(
                "订阅地址必须以 .js 或 .json 结尾".to_string(),
            ));
        }
        let existing: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM music_plugin_subscription WHERE url = ?1")
                .bind(url)
                .fetch_optional(&self.pool)
                .await?;
        if existing.is_some() {
            return Err(AppError::BadRequest("该订阅地址已存在".to_string()));
        }
        let name = if name.trim().is_empty() {
            url.rsplit('/').next().unwrap_or(url).to_string()
        } else {
            name.trim().to_string()
        };
        let now = chrono::Utc::now().timestamp_millis();
        let result = sqlx::query(
            "INSERT INTO music_plugin_subscription (user_ns, name, url, created_at)
             VALUES ('default', ?1, ?2, ?3)",
        )
        .bind(&name)
        .bind(url)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(PluginSubscription {
            id: result.last_insert_rowid(),
            name,
            url: url.to_string(),
            created_at: now,
        })
    }

    pub async fn remove_subscription(&self, id: i64) -> Result<bool, AppError> {
        let result = sqlx::query("DELETE FROM music_plugin_subscription WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// 拉取单个订阅（`.js` 清单或 `.json` 清单都可能）。
    pub async fn update_subscription(
        &self,
        id: i64,
    ) -> Result<Vec<PluginInstallResult>, AppError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT url FROM music_plugin_subscription WHERE id = ?1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let (url,) = row.ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
        self.install_from_urls(&url).await
    }

    /// 更新全部订阅源，结果合并返回。
    pub async fn update_all_subscriptions(
        &self,
    ) -> Result<Vec<PluginInstallResult>, AppError> {
        let mut results = Vec::new();
        for subscription in self.subscriptions().await? {
            match self.install_from_urls(&subscription.url).await {
                Ok(mut items) => results.append(&mut items),
                Err(err) => results.push(PluginInstallResult {
                    url: subscription.url,
                    success: false,
                    error: err.to_string(),
                    ..Default::default()
                }),
            }
        }
        Ok(results)
    }

    pub async fn set_enabled(&self, platform: &str, enabled: bool) -> Result<bool, AppError> {
        if !enabled {
            self.drop_worker(platform);
        }
        let result = sqlx::query(
            "UPDATE music_plugin SET enabled = ?2, updated_at = ?3 WHERE platform = ?1",
        )
        .bind(platform)
        .bind(if enabled { 1 } else { 0 })
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("插件 {platform} 未安装")));
        }
        Ok(enabled)
    }

    /// 读取已启用插件的脚本源码；文件缺失时返回可读错误。
    pub async fn enabled_scripts(&self) -> Result<Vec<(MusicPlugin, String)>, AppError> {
        let mut result = Vec::new();
        for plugin in self.list().await? {
            if !plugin.enabled {
                continue;
            }
            match std::fs::read_to_string(&plugin.file_path) {
                Ok(source) => result.push((plugin, source)),
                Err(err) => {
                    tracing::warn!("音乐插件文件不可读({}): {err}", plugin.platform);
                    let _ = self.set_enabled(&plugin.platform, false).await;
                }
            }
        }
        Ok(result)
    }
}

fn row_to_plugin(row: sqlx::sqlite::SqliteRow) -> MusicPlugin {
    MusicPlugin {
        platform: row.get("platform"),
        name: row.get("name"),
        version: row.get("version"),
        author: row.get("author"),
        src_url: row.get("src_url"),
        file_path: row.get("file_path"),
        enabled: row.get::<i64, _>("enabled") != 0,
        description: row.get("description"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

/// 语义版本比较(仅数字段, 缺省段视为 0)。返回 -1/0/1。
pub fn version_cmp(left: &str, right: &str) -> i32 {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split('.')
            .map(|part| {
                part.trim()
                    .chars()
                    .take_while(|ch| ch.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u64>()
                    .unwrap_or(0)
            })
            .collect()
    };
    let a = parse(left);
    let b = parse(right);
    let len = a.len().max(b.len());
    for index in 0..len {
        let left_part = a.get(index).copied().unwrap_or(0);
        let right_part = b.get(index).copied().unwrap_or(0);
        if left_part != right_part {
            return if left_part > right_part { 1 } else { -1 };
        }
    }
    0
}

/// 文件名只保留 ASCII 安全字符；中文平台名会退化成下划线, 由 uuid 保证唯一。
fn sanitize_file_stem(platform: &str) -> String {
    let stem: String = platform
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = stem.trim_matches('_');
    if trimmed.is_empty() {
        "plugin".to_string()
    } else {
        trimmed.chars().take(32).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert_eq!(version_cmp("1.2.4", "1.2.3"), 1);
        assert_eq!(version_cmp("2.0.0", "1.99.99"), 1);
        assert_eq!(version_cmp("1.0", "1.0.0"), 0);
        assert_eq!(version_cmp("0.9", "1.0.0"), -1);
    }

    #[test]
    fn sanitizes_platform_for_file_name() {
        assert_eq!(sanitize_file_stem("测试音源"), "plugin");
        assert_eq!(sanitize_file_stem("demo-plugin_1"), "demo-plugin_1");
    }

    async fn test_registry() -> (PluginRegistry, PathBuf) {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open memory db");
        sqlx::migrate!("src/storage/db/migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        let dir = std::env::temp_dir().join(format!("reader-plugin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let http = HttpClient::new(5, None).expect("http client");
        let registry = PluginRegistry::new(pool, dir.to_str().expect("utf8 path"), http);
        (registry, dir)
    }

    fn plugin_script(platform: &str, version: &str, src_url: &str) -> String {
        format!(
            r#"module.exports = {{ platform: "{platform}", name: "{platform}", version: "{version}", srcUrl: "{src_url}" }};"#
        )
    }

    #[tokio::test]
    async fn allows_same_version_reinstall_but_rejects_downgrade() {
        let (registry, dir) = test_registry().await;
        registry
            .install(&plugin_script("demo", "1.1.0", ""), None)
            .await
            .expect("first install");
        // 同版本允许覆盖安装。
        registry
            .install(
                &plugin_script("demo", "1.1.0", "https://example.com/demo.js"),
                None,
            )
            .await
            .expect("reinstall same version");
        assert_eq!(
            registry.get("demo").await.expect("get").expect("plugin").src_url,
            "https://example.com/demo.js"
        );
        // 降级被拒绝。
        assert!(registry
            .install(&plugin_script("demo", "1.0.0", ""), None)
            .await
            .is_err());
        // 本地重装（源码没写 srcUrl）不应丢掉已有的更新地址。
        registry
            .install(&plugin_script("demo", "1.1.0", ""), None)
            .await
            .expect("reinstall local");
        assert_eq!(
            registry.get("demo").await.expect("get").expect("plugin").src_url,
            "https://example.com/demo.js"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn subscriptions_validate_urls_and_reject_duplicates() {
        let (registry, dir) = test_registry().await;
        let first = registry
            .add_subscription("测试订阅", "https://example.com/plugins.json")
            .await
            .expect("add subscription");
        assert_eq!(first.name, "测试订阅");
        assert!(registry
            .add_subscription("", "https://example.com/plugins.json")
            .await
            .is_err());
        assert!(registry
            .add_subscription("bad", "https://example.com/plugins.zip")
            .await
            .is_err());

        // 未填名称时用文件名兜底。
        let second = registry
            .add_subscription("", "https://example.com/single.js")
            .await
            .expect("add js subscription");
        assert_eq!(second.name, "single.js");
        assert_eq!(registry.subscriptions().await.expect("list").len(), 2);

        assert!(registry
            .remove_subscription(first.id)
            .await
            .expect("remove"));
        assert_eq!(registry.subscriptions().await.expect("list").len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn reorders_plugins_by_given_order() {
        let (registry, dir) = test_registry().await;
        for platform in ["a", "b", "c"] {
            registry
                .install(&plugin_script(platform, "1.0.0", ""), None)
                .await
                .expect("install");
        }
        registry
            .reorder(&["c".to_string(), "a".to_string(), "b".to_string()])
            .await
            .expect("reorder");
        let order: Vec<String> = registry
            .list()
            .await
            .expect("list")
            .into_iter()
            .map(|plugin| plugin.platform)
            .collect();
        assert_eq!(order, vec!["c", "a", "b"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn uninstall_all_removes_records_and_files() {
        let (registry, dir) = test_registry().await;
        for platform in ["a", "b"] {
            registry
                .install(&plugin_script(platform, "1.0.0", ""), None)
                .await
                .expect("install");
        }
        let files: Vec<PathBuf> = registry
            .list()
            .await
            .expect("list")
            .iter()
            .map(|plugin| PathBuf::from(&plugin.file_path))
            .collect();
        assert_eq!(registry.uninstall_all().await.expect("uninstall"), 2);
        assert!(registry.list().await.expect("list").is_empty());
        assert!(files.iter().all(|path| !path.exists()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
