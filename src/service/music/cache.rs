//! 音乐播放缓存。
//!
//! 需要 Referer/UA 的音源原本就要整块取回字节（`reader://musicStream`），
//! 这里把取回的音频按 `url + headers` 的哈希落盘，重复播放时直接命中，
//! 避免每次都重新走网络。超过容量上限按最久未使用淘汰。
//!
//! 直链音源仍然走 `<audio>` 流式播放，不经过本缓存，避免失去 Range 拖动。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// 默认上限 512MB。
pub const DEFAULT_CACHE_LIMIT: u64 = 512 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheMeta {
    content_type: Option<String>,
    size: u64,
}

#[derive(Clone)]
pub struct MusicCache {
    dir: PathBuf,
    limit_bytes: u64,
}

impl MusicCache {
    pub fn new(storage_dir: &str, limit_bytes: u64) -> Self {
        let dir = Path::new(storage_dir).join("music_cache");
        if let Err(err) = std::fs::create_dir_all(&dir) {
            tracing::warn!("创建音乐缓存目录失败: {err}");
        }
        Self { dir, limit_bytes }
    }

    /// 缓存键：url 与请求头都参与，避免同一地址不同 Referer 拿到错误内容。
    fn key(url: &str, headers: &Map<String, Value>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let mut names: Vec<&String> = headers.keys().collect();
        names.sort();
        for name in names {
            hasher.update(b"\0");
            hasher.update(name.as_bytes());
            hasher.update(b"\0");
            let value = headers
                .get(name)
                .map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| item.to_string())
                })
                .unwrap_or_default();
            hasher.update(value.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    fn bin_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.bin"))
    }

    fn meta_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.meta"))
    }

    /// 命中时返回字节与 Content-Type，并刷新访问时间（重写 meta，更新 mtime）。
    pub fn lookup(
        &self,
        url: &str,
        headers: &Map<String, Value>,
    ) -> Option<(Vec<u8>, Option<String>)> {
        let key = Self::key(url, headers);
        let bytes = std::fs::read(self.bin_path(&key)).ok()?;
        let meta_path = self.meta_path(&key);
        let raw = std::fs::read_to_string(&meta_path).ok()?;
        let meta: CacheMeta = serde_json::from_str(&raw).ok()?;
        let _ = std::fs::write(&meta_path, &raw);
        Some((bytes, meta.content_type))
    }

    /// 写入缓存并触发淘汰；磁盘异常只记日志，不影响播放。
    pub fn store(&self, url: &str, headers: &Map<String, Value>, bytes: &[u8], content_type: Option<&str>) {
        let key = Self::key(url, headers);
        let bin_path = self.bin_path(&key);
        if let Err(err) = std::fs::write(&bin_path, bytes) {
            tracing::warn!("写入音乐缓存失败: {err}");
            return;
        }
        let payload = serde_json::to_string(&CacheMeta {
            content_type: content_type.map(str::to_string),
            size: bytes.len() as u64,
        })
        .unwrap_or_default();
        let _ = std::fs::write(self.meta_path(&key), payload);
        self.evict(&key);
    }

    /// 按 mtime 从旧到新删除，直到容量回落到上限以内。
    fn evict(&self, keep: &str) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut items: Vec<(PathBuf, PathBuf, u64, SystemTime)> = Vec::new();
        let mut total = 0u64;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("meta") {
                continue;
            }
            let Some(key) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(meta) = serde_json::from_str::<CacheMeta>(&raw) else {
                continue;
            };
            total += meta.size;
            if key == keep {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|info| info.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            items.push((path.clone(), self.bin_path(key), meta.size, modified));
        }
        if total <= self.limit_bytes {
            return;
        }
        items.sort_by_key(|item| item.3);
        for (meta_path, bin_path, size, _) in items {
            if total <= self.limit_bytes {
                break;
            }
            let _ = std::fs::remove_file(&bin_path);
            let _ = std::fs::remove_file(&meta_path);
            total = total.saturating_sub(size);
        }
    }

    /// 当前缓存占用的字节数与文件数。
    pub fn stats(&self) -> (u64, usize) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return (0, 0);
        };
        let mut total = 0u64;
        let mut files = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("meta") {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(meta) = serde_json::from_str::<CacheMeta>(&raw) {
                total += meta.size;
                files += 1;
            }
        }
        (total, files)
    }

    /// 清空缓存，返回释放的字节数。
    pub fn clear(&self) -> u64 {
        let (freed, _) = self.stats();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return 0;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match path.extension().and_then(|ext| ext.to_str()) {
                Some("bin") | Some("meta") => {
                    let _ = std::fs::remove_file(path);
                }
                _ => {}
            }
        }
        freed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "reader-music-cache-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn stores_and_reads_back_bytes() {
        let root = temp_dir("roundtrip");
        let cache = MusicCache::new(root.to_str().unwrap(), DEFAULT_CACHE_LIMIT);
        let headers = Map::new();
        assert!(cache.lookup("https://a/b.mp3", &headers).is_none());

        cache.store("https://a/b.mp3", &headers, b"audio-bytes", Some("audio/mpeg"));
        let (bytes, content_type) = cache.lookup("https://a/b.mp3", &headers).unwrap();
        assert_eq!(bytes, b"audio-bytes");
        assert_eq!(content_type.as_deref(), Some("audio/mpeg"));

        // 不同 headers 视为不同缓存项。
        let mut other = Map::new();
        other.insert("Referer".to_string(), json!("https://a"));
        assert!(cache.lookup("https://a/b.mp3", &other).is_none());

        assert_eq!(cache.stats(), (11, 1));
        assert_eq!(cache.clear(), 11);
        assert_eq!(cache.stats(), (0, 0));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn evicts_oldest_entries_over_limit() {
        let root = temp_dir("evict");
        let cache = MusicCache::new(root.to_str().unwrap(), 10);
        let headers = Map::new();
        cache.store("https://a/1.mp3", &headers, b"123456", None);
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.store("https://a/2.mp3", &headers, b"789012", None);
        // 12 字节超过 10 字节上限，最旧的 1 被淘汰。
        let (bytes, _) = cache.stats();
        assert!(bytes <= 10);
        assert!(cache.lookup("https://a/1.mp3", &headers).is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}
