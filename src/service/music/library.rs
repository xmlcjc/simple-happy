//! 音乐曲库：收藏、自定义歌单、播放历史与每日听歌统计。
//!
//! 全部为本地数据, 沿用 `user_ns` 维度（当前固定 `default`）。歌曲以
//! `item_json` 原样保存, 这样插件私有字段在重新播放/取歌词时不会丢。

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::error::AppError;
use crate::service::music::model::MusicItem;

#[derive(Clone)]
pub struct MusicLibrary {
    pool: SqlitePool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlaylistSummary {
    pub id: String,
    pub name: String,
    pub item_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicHistoryEntry {
    pub item: MusicItem,
    pub started_at: i64,
    pub played_seconds: i64,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicDailyStat {
    pub day: String,
    pub seconds: i64,
    pub plays: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicStatsOverview {
    pub total_seconds: i64,
    pub today_seconds: i64,
    pub total_plays: i64,
    pub daily: Vec<MusicDailyStat>,
}

impl MusicLibrary {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ── 收藏 ──

    /// 切换收藏状态, 返回切换后是否已收藏。
    pub async fn toggle_favorite(
        &self,
        user_ns: &str,
        item: &MusicItem,
    ) -> Result<bool, AppError> {
        if item.id.trim().is_empty() || item.platform.trim().is_empty() {
            return Err(AppError::BadRequest("歌曲缺少 platform/id".to_string()));
        }
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM music_favorite WHERE user_ns = ?1 AND platform = ?2 AND item_id = ?3",
        )
        .bind(user_ns)
        .bind(&item.platform)
        .bind(&item.id)
        .fetch_optional(&self.pool)
        .await?;
        if exists.is_some() {
            sqlx::query(
                "DELETE FROM music_favorite WHERE user_ns = ?1 AND platform = ?2 AND item_id = ?3",
            )
            .bind(user_ns)
            .bind(&item.platform)
            .bind(&item.id)
            .execute(&self.pool)
            .await?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO music_favorite (user_ns, platform, item_id, item_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(user_ns)
        .bind(&item.platform)
        .bind(&item.id)
        .bind(item.to_json())
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&self.pool)
        .await?;
        Ok(true)
    }

    pub async fn is_favorite(
        &self,
        user_ns: &str,
        platform: &str,
        item_id: &str,
    ) -> Result<bool, AppError> {
        let row: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM music_favorite WHERE user_ns = ?1 AND platform = ?2 AND item_id = ?3",
        )
        .bind(user_ns)
        .bind(platform)
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    pub async fn favorites(&self, user_ns: &str) -> Result<Vec<MusicItem>, AppError> {
        let rows = sqlx::query(
            "SELECT item_json FROM music_favorite WHERE user_ns = ?1 ORDER BY created_at DESC",
        )
        .bind(user_ns)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| parse_item(row.get::<String, _>("item_json")))
            .collect())
    }

    // ── 歌单 ──

    pub async fn create_playlist(
        &self,
        user_ns: &str,
        name: &str,
    ) -> Result<MusicPlaylistSummary, AppError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest("歌单名称不能为空".to_string()));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO music_playlist (user_ns, id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(user_ns)
        .bind(&id)
        .bind(name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(MusicPlaylistSummary {
            id,
            name: name.to_string(),
            item_count: 0,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn rename_playlist(
        &self,
        user_ns: &str,
        id: &str,
        name: &str,
    ) -> Result<(), AppError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest("歌单名称不能为空".to_string()));
        }
        let result = sqlx::query(
            "UPDATE music_playlist SET name = ?3, updated_at = ?4 WHERE user_ns = ?1 AND id = ?2",
        )
        .bind(user_ns)
        .bind(id)
        .bind(name)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound("歌单不存在".to_string()));
        }
        Ok(())
    }

    pub async fn delete_playlist(&self, user_ns: &str, id: &str) -> Result<bool, AppError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM music_playlist_item WHERE user_ns = ?1 AND playlist_id = ?2")
            .bind(user_ns)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM music_playlist WHERE user_ns = ?1 AND id = ?2")
            .bind(user_ns)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn playlists(&self, user_ns: &str) -> Result<Vec<MusicPlaylistSummary>, AppError> {
        let rows = sqlx::query(
            "SELECT p.id, p.name, p.created_at, p.updated_at,
                    (SELECT COUNT(*) FROM music_playlist_item i
                      WHERE i.user_ns = p.user_ns AND i.playlist_id = p.id) AS item_count
             FROM music_playlist p
             WHERE p.user_ns = ?1
             ORDER BY p.created_at ASC",
        )
        .bind(user_ns)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| MusicPlaylistSummary {
                id: row.get("id"),
                name: row.get("name"),
                item_count: row.get("item_count"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect())
    }

    pub async fn playlist_items(
        &self,
        user_ns: &str,
        playlist_id: &str,
    ) -> Result<Vec<MusicItem>, AppError> {
        let rows = sqlx::query(
            "SELECT item_json FROM music_playlist_item
             WHERE user_ns = ?1 AND playlist_id = ?2
             ORDER BY sort_order ASC, added_at ASC",
        )
        .bind(user_ns)
        .bind(playlist_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| parse_item(row.get::<String, _>("item_json")))
            .collect())
    }

    /// 批量加入歌单; 已存在的歌曲会被忽略, 不改变原有顺序。
    pub async fn add_to_playlist(
        &self,
        user_ns: &str,
        playlist_id: &str,
        items: &[MusicItem],
    ) -> Result<i64, AppError> {
        if items.is_empty() {
            return Ok(0);
        }
        self.ensure_playlist(user_ns, playlist_id).await?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut added = 0;
        let mut tx = self.pool.begin().await?;
        let max_order: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(sort_order) FROM music_playlist_item WHERE user_ns = ?1 AND playlist_id = ?2",
        )
        .bind(user_ns)
        .bind(playlist_id)
        .fetch_one(&mut *tx)
        .await?;
        let mut order = max_order.unwrap_or(-1);
        for item in items {
            if item.id.trim().is_empty() || item.platform.trim().is_empty() {
                continue;
            }
            order += 1;
            let result = sqlx::query(
                "INSERT INTO music_playlist_item
                   (user_ns, playlist_id, platform, item_id, item_json, sort_order, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(user_ns, playlist_id, platform, item_id) DO NOTHING",
            )
            .bind(user_ns)
            .bind(playlist_id)
            .bind(&item.platform)
            .bind(&item.id)
            .bind(item.to_json())
            .bind(order)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            added += result.rows_affected() as i64;
        }
        sqlx::query("UPDATE music_playlist SET updated_at = ?3 WHERE user_ns = ?1 AND id = ?2")
            .bind(user_ns)
            .bind(playlist_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(added)
    }

    pub async fn remove_from_playlist(
        &self,
        user_ns: &str,
        playlist_id: &str,
        platform: &str,
        item_id: &str,
    ) -> Result<bool, AppError> {
        let result = sqlx::query(
            "DELETE FROM music_playlist_item
             WHERE user_ns = ?1 AND playlist_id = ?2 AND platform = ?3 AND item_id = ?4",
        )
        .bind(user_ns)
        .bind(playlist_id)
        .bind(platform)
        .bind(item_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// 批量从歌单移除，单事务完成，避免逐首删除的半途状态。
    pub async fn remove_many_from_playlist(
        &self,
        user_ns: &str,
        playlist_id: &str,
        keys: &[(String, String)],
    ) -> Result<u64, AppError> {
        if keys.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await?;
        let mut removed = 0u64;
        for (platform, item_id) in keys {
            let result = sqlx::query(
                "DELETE FROM music_playlist_item
                 WHERE user_ns = ?1 AND playlist_id = ?2 AND platform = ?3 AND item_id = ?4",
            )
            .bind(user_ns)
            .bind(playlist_id)
            .bind(platform)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
            removed += result.rows_affected();
        }
        sqlx::query("UPDATE music_playlist SET updated_at = ?1 WHERE user_ns = ?2 AND id = ?3")
            .bind(chrono::Utc::now().timestamp_millis())
            .bind(user_ns)
            .bind(playlist_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(removed)
    }

    /// 按给定顺序重排歌单项。
    pub async fn reorder_playlist(
        &self,
        user_ns: &str,
        playlist_id: &str,
        keys: &[(String, String)],
    ) -> Result<(), AppError> {
        self.ensure_playlist(user_ns, playlist_id).await?;
        let mut tx = self.pool.begin().await?;
        for (index, (platform, item_id)) in keys.iter().enumerate() {
            sqlx::query(
                "UPDATE music_playlist_item SET sort_order = ?5
                 WHERE user_ns = ?1 AND playlist_id = ?2 AND platform = ?3 AND item_id = ?4",
            )
            .bind(user_ns)
            .bind(playlist_id)
            .bind(platform)
            .bind(item_id)
            .bind(index as i64)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("UPDATE music_playlist SET updated_at = ?3 WHERE user_ns = ?1 AND id = ?2")
            .bind(user_ns)
            .bind(playlist_id)
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn ensure_playlist(&self, user_ns: &str, playlist_id: &str) -> Result<(), AppError> {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM music_playlist WHERE user_ns = ?1 AND id = ?2")
                .bind(user_ns)
                .bind(playlist_id)
                .fetch_optional(&self.pool)
                .await?;
        if exists.is_none() {
            return Err(AppError::NotFound("歌单不存在".to_string()));
        }
        Ok(())
    }

    // ── 历史与统计 ──

    /// 记录一次播放: 累加时长, 更新最近播放时间, 并写入当日统计。
    pub async fn record_history(
        &self,
        user_ns: &str,
        item: &MusicItem,
        seconds: i64,
        finished: bool,
    ) -> Result<(), AppError> {
        if item.id.trim().is_empty() || item.platform.trim().is_empty() {
            return Err(AppError::BadRequest("歌曲缺少 platform/id".to_string()));
        }
        let seconds = seconds.max(0);
        let now = chrono::Utc::now().timestamp_millis();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO music_history
               (user_ns, platform, item_id, item_json, started_at, played_seconds, finished)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(user_ns, platform, item_id) DO UPDATE SET
               item_json = excluded.item_json,
               started_at = excluded.started_at,
               played_seconds = music_history.played_seconds + excluded.played_seconds,
               finished = MAX(music_history.finished, excluded.finished)",
        )
        .bind(user_ns)
        .bind(&item.platform)
        .bind(&item.id)
        .bind(item.to_json())
        .bind(now)
        .bind(seconds)
        .bind(if finished { 1 } else { 0 })
        .execute(&mut *tx)
        .await?;

        if seconds > 0 {
            let day = chrono::Local::now().format("%Y-%m-%d").to_string();
            sqlx::query(
                "INSERT INTO music_daily_stat (user_ns, day, seconds, plays)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(user_ns, day) DO UPDATE SET
                   seconds = music_daily_stat.seconds + excluded.seconds,
                   plays = music_daily_stat.plays + excluded.plays",
            )
            .bind(user_ns)
            .bind(&day)
            .bind(seconds)
            .bind(if finished { 1 } else { 0 })
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn history(
        &self,
        user_ns: &str,
        limit: u32,
    ) -> Result<Vec<MusicHistoryEntry>, AppError> {
        let rows = sqlx::query(
            "SELECT item_json, started_at, played_seconds, finished
             FROM music_history WHERE user_ns = ?1
             ORDER BY started_at DESC LIMIT ?2",
        )
        .bind(user_ns)
        .bind(limit.clamp(1, 500) as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let item = parse_item(row.get::<String, _>("item_json"))?;
                Some(MusicHistoryEntry {
                    item,
                    started_at: row.get("started_at"),
                    played_seconds: row.get("played_seconds"),
                    finished: row.get::<i64, _>("finished") != 0,
                })
            })
            .collect())
    }

    pub async fn clear_history(&self, user_ns: &str) -> Result<u64, AppError> {
        let result = sqlx::query("DELETE FROM music_history WHERE user_ns = ?1")
            .bind(user_ns)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn stats_overview(
        &self,
        user_ns: &str,
        days: u32,
    ) -> Result<MusicStatsOverview, AppError> {
        let days = days.clamp(1, 365);
        let rows = sqlx::query(
            "SELECT day, seconds, plays FROM music_daily_stat
             WHERE user_ns = ?1 ORDER BY day DESC LIMIT ?2",
        )
        .bind(user_ns)
        .bind(days as i64)
        .fetch_all(&self.pool)
        .await?;
        let daily: Vec<MusicDailyStat> = rows
            .into_iter()
            .map(|row| MusicDailyStat {
                day: row.get("day"),
                seconds: row.get("seconds"),
                plays: row.get("plays"),
            })
            .collect();
        let total_seconds = daily.iter().map(|item| item.seconds).sum();
        let total_plays = daily.iter().map(|item| item.plays).sum();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let today_seconds = daily
            .iter()
            .find(|item| item.day == today)
            .map(|item| item.seconds)
            .unwrap_or(0);
        Ok(MusicStatsOverview {
            total_seconds,
            today_seconds,
            total_plays,
            daily,
        })
    }
}

fn parse_item(raw: String) -> Option<MusicItem> {
    serde_json::from_str::<MusicItem>(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db;

    async fn memory_pool() -> SqlitePool {
        let pool = db::init_pool("sqlite::memory:").await.expect("pool");
        pool
    }

    fn item(id: &str) -> MusicItem {
        MusicItem {
            platform: "demo".to_string(),
            id: id.to_string(),
            title: format!("歌曲{id}"),
            artist: "歌手".to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn favorites_toggle_and_list() {
        let library = MusicLibrary::new(memory_pool().await);
        assert!(!library.is_favorite("default", "demo", "1").await.unwrap());
        assert!(library.toggle_favorite("default", &item("1")).await.unwrap());
        assert!(library.is_favorite("default", "demo", "1").await.unwrap());
        assert_eq!(library.favorites("default").await.unwrap().len(), 1);
        assert!(!library.toggle_favorite("default", &item("1")).await.unwrap());
        assert!(library.favorites("default").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn playlist_add_dedup_and_reorder() {
        let library = MusicLibrary::new(memory_pool().await);
        let playlist = library.create_playlist("default", "我的歌单").await.unwrap();
        let added = library
            .add_to_playlist("default", &playlist.id, &[item("1"), item("2"), item("1")])
            .await
            .unwrap();
        assert_eq!(added, 2);
        let items = library.playlist_items("default", &playlist.id).await.unwrap();
        assert_eq!(items.len(), 2);

        library
            .reorder_playlist(
                "default",
                &playlist.id,
                &[("demo".into(), "2".into()), ("demo".into(), "1".into())],
            )
            .await
            .unwrap();
        let reordered = library.playlist_items("default", &playlist.id).await.unwrap();
        assert_eq!(reordered[0].id, "2");

        assert!(library
            .remove_from_playlist("default", &playlist.id, "demo", "2")
            .await
            .unwrap());
        assert_eq!(library.playlist_items("default", &playlist.id).await.unwrap().len(), 1);
        assert!(library.delete_playlist("default", &playlist.id).await.unwrap());
    }

    #[tokio::test]
    async fn playlist_remove_batch_deletes_selected_items() {
        let library = MusicLibrary::new(memory_pool().await);
        let playlist = library.create_playlist("default", "批量").await.unwrap();
        let added = library
            .add_to_playlist("default", &playlist.id, &[item("1"), item("2"), item("3")])
            .await
            .unwrap();
        assert_eq!(added, 3);

        let removed = library
            .remove_many_from_playlist(
                "default",
                &playlist.id,
                &[("demo".into(), "1".into()), ("demo".into(), "3".into())],
            )
            .await
            .unwrap();
        assert_eq!(removed, 2);
        let items = library.playlist_items("default", &playlist.id).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "2");

        // 空列表直接返回 0，不会误删整个歌单。
        assert_eq!(
            library
                .remove_many_from_playlist("default", &playlist.id, &[])
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn history_and_daily_stats_accumulate() {
        let library = MusicLibrary::new(memory_pool().await);
        library.record_history("default", &item("1"), 30, false).await.unwrap();
        library.record_history("default", &item("1"), 20, true).await.unwrap();
        let history = library.history("default", 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].played_seconds, 50);
        assert!(history[0].finished);

        let stats = library.stats_overview("default", 7).await.unwrap();
        assert_eq!(stats.total_seconds, 50);
        assert_eq!(stats.total_plays, 1);
        assert_eq!(stats.today_seconds, 50);

        assert_eq!(library.clear_history("default").await.unwrap(), 1);
        assert!(library.history("default", 10).await.unwrap().is_empty());
    }
}
