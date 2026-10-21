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
use crate::service::music::model::{MusicPlugin, PluginVariable};

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
            "SELECT platform, name, version, author, src_url, file_path, enabled, created_at, updated_at
             FROM music_plugin ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_plugin).collect())
    }

    pub async fn get(&self, platform: &str) -> Result<Option<MusicPlugin>, AppError> {
        let row = sqlx::query(
            "SELECT platform, name, version, author, src_url, file_path, enabled, created_at, updated_at
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
        let manifest = parse_manifest(source)?;
        let existing = self.get(&manifest.platform).await?;
        if let Some(current) = &existing {
            if version_cmp(&manifest.version, &current.version) <= 0 {
                return Err(AppError::BadRequest(format!(
                    "已安装相同或更高版本的插件（{} {}）",
                    current.platform, current.version
                )));
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
               (platform, name, version, author, src_url, file_path, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8)
             ON CONFLICT(platform) DO UPDATE SET
               name = excluded.name,
               version = excluded.version,
               author = excluded.author,
               src_url = excluded.src_url,
               file_path = excluded.file_path,
               enabled = 1,
               updated_at = excluded.updated_at",
        )
        .bind(&manifest.platform)
        .bind(&display_name)
        .bind(&manifest.version)
        .bind(&manifest.author)
        .bind(&manifest.src_url)
        .bind(&path_text)
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
        let manifest = parse_manifest(&source)?;
        // 防止 srcUrl 指向别的插件，覆盖掉当前插件。
        if manifest.platform != platform {
            return Err(AppError::BadRequest(format!(
                "更新文件属于插件 {}，与当前插件不一致，已拒绝覆盖",
                manifest.platform
            )));
        }
        std::fs::write(&current.file_path, &source)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("写入插件文件失败: {e}")))?;
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE music_plugin SET name = ?2, version = ?3, author = ?4, src_url = ?5, updated_at = ?6
             WHERE platform = ?1",
        )
        .bind(platform)
        .bind(&manifest.name)
        .bind(&manifest.version)
        .bind(&manifest.author)
        .bind(&manifest.src_url)
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
}
