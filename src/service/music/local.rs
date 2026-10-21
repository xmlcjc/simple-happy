//! 本地音乐曲库：导入、元数据解析与列表。
//!
//! 只保存本地路径与解析出的展示信息, 音频字节仍由原文件读取。
//! 播放时通过 `reader://musicLocal?path=...` 访问, 该路由只放行数据库里
//! 已登记过的路径, 避免任意文件读取。

use std::path::Path;

use lofty::prelude::*;
use lofty::probe::Probe;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::error::AppError;

const SUPPORTED_EXTENSIONS: [&str; 7] = ["mp3", "flac", "m4a", "aac", "ogg", "wav", "opus"];
const MAX_IMPORT: usize = 2000;

#[derive(Clone)]
pub struct MusicLocalLibrary {
    pool: SqlitePool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTrack {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub format: String,
    pub size: i64,
    pub mtime: i64,
    pub added_at: i64,
}

impl MusicLocalLibrary {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 导入一批本地文件。目录会被展开为其中的音频文件。
    pub async fn import_paths(
        &self,
        user_ns: &str,
        paths: &[String],
    ) -> Result<Vec<LocalTrack>, AppError> {
        let mut files = Vec::new();
        for raw in paths.iter().take(MAX_IMPORT) {
            let path = Path::new(raw.trim());
            if path.is_dir() {
                collect_dir(path, &mut files)?;
                continue;
            }
            if is_supported(path) {
                files.push(path.to_path_buf());
            }
        }
        files.truncate(MAX_IMPORT);

        let mut imported = Vec::new();
        for path in files {
            match self.import_one(user_ns, &path).await {
                Ok(track) => imported.push(track),
                Err(err) => tracing::warn!("跳过本地音乐 {}: {err}", path.display()),
            }
        }
        Ok(imported)
    }

    async fn import_one(&self, user_ns: &str, path: &Path) -> Result<LocalTrack, AppError> {
        let text = path.to_string_lossy().to_string();
        let metadata = std::fs::metadata(path)
            .map_err(|e| AppError::BadRequest(format!("无法读取文件: {e}")))?;
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_millis() as i64)
            .unwrap_or(0);
        let tags = read_tags(path);
        let fallback_title = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("未知歌曲")
            .to_string();
        let track = LocalTrack {
            path: text.clone(),
            title: tags.title.unwrap_or(fallback_title),
            artist: tags.artist.unwrap_or_default(),
            album: tags.album.unwrap_or_default(),
            duration_ms: tags.duration_ms,
            format: path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase(),
            size: metadata.len() as i64,
            mtime,
            added_at: chrono::Utc::now().timestamp_millis(),
        };

        sqlx::query(
            "INSERT INTO music_local_track
               (user_ns, path, title, artist, album, duration_ms, format, size, mtime, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(user_ns, path) DO UPDATE SET
               title = excluded.title,
               artist = excluded.artist,
               album = excluded.album,
               duration_ms = excluded.duration_ms,
               format = excluded.format,
               size = excluded.size,
               mtime = excluded.mtime",
        )
        .bind(user_ns)
        .bind(&track.path)
        .bind(&track.title)
        .bind(&track.artist)
        .bind(&track.album)
        .bind(track.duration_ms)
        .bind(&track.format)
        .bind(track.size)
        .bind(track.mtime)
        .bind(track.added_at)
        .execute(&self.pool)
        .await?;
        Ok(track)
    }

    pub async fn list(&self, user_ns: &str) -> Result<Vec<LocalTrack>, AppError> {
        let rows = sqlx::query(
            "SELECT path, title, artist, album, duration_ms, format, size, mtime, added_at
             FROM music_local_track WHERE user_ns = ?1
             ORDER BY added_at DESC",
        )
        .bind(user_ns)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_track).collect())
    }

    pub async fn remove(&self, user_ns: &str, path: &str) -> Result<bool, AppError> {
        let result =
            sqlx::query("DELETE FROM music_local_track WHERE user_ns = ?1 AND path = ?2")
                .bind(user_ns)
                .bind(path)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    /// 播放前的白名单校验: 只允许已登记的路径。
    pub async fn find(&self, user_ns: &str, path: &str) -> Result<Option<LocalTrack>, AppError> {
        let row = sqlx::query(
            "SELECT path, title, artist, album, duration_ms, format, size, mtime, added_at
             FROM music_local_track WHERE user_ns = ?1 AND path = ?2",
        )
        .bind(user_ns)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_track))
    }
}

#[derive(Default)]
struct Tags {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration_ms: i64,
}

fn read_tags(path: &Path) -> Tags {
    let mut tags = Tags::default();
    let Ok(tagged) = Probe::open(path).and_then(|probe| probe.read()) else {
        return tags;
    };
    tags.duration_ms = tagged.properties().duration().as_millis() as i64;
    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        tags.title = tag.title().map(|value| value.to_string());
        tags.artist = tag.artist().map(|value| value.to_string());
        tags.album = tag.album().map(|value| value.to_string());
    }
    tags
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn collect_dir(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| AppError::BadRequest(format!("无法读取目录: {e}")))?;
    for entry in entries.flatten() {
        if out.len() >= MAX_IMPORT {
            break;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_dir(&path, out)?;
        } else if is_supported(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// 本地音频路由的 Content-Type。
pub fn audio_mime(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

fn row_to_track(row: sqlx::sqlite::SqliteRow) -> LocalTrack {
    LocalTrack {
        path: row.get("path"),
        title: row.get("title"),
        artist: row.get("artist"),
        album: row.get("album"),
        duration_ms: row.get("duration_ms"),
        format: row.get("format"),
        size: row.get("size"),
        mtime: row.get("mtime"),
        added_at: row.get("added_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db;

    #[tokio::test]
    async fn imports_falls_back_to_file_name_and_lists() {
        let dir = std::env::temp_dir().join(format!("music-local-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("测试歌曲.mp3");
        // 空文件不是有效音频, 但导入应回退到文件名而不是整体失败。
        std::fs::write(&file, b"not really mp3").unwrap();

        let pool = db::init_pool("sqlite::memory:").await.unwrap();
        let library = MusicLocalLibrary::new(pool);
        let imported = library
            .import_paths("default", &[file.to_string_lossy().to_string()])
            .await
            .unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].title, "测试歌曲");

        let list = library.list("default").await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(library
            .find("default", &file.to_string_lossy())
            .await
            .unwrap()
            .is_some());
        assert!(library.remove("default", &file.to_string_lossy()).await.unwrap());
        assert!(library.list("default").await.unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn accepts_only_supported_extensions() {
        assert!(is_supported(Path::new("a/b.flac")));
        assert!(is_supported(Path::new("a/b.MP3")));
        assert!(!is_supported(Path::new("a/b.txt")));
    }
}
