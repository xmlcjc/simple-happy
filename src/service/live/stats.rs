//! 直播观看统计。
//!
//! 与阅读统计同构(按 `user_ns + date` 聚合), 但多一维房间: 房间页按固定间隔
//! 上报观看秒数, 这里累加。`sessions` 只在"进入房间开始观看"时 +1, 用来区分
//! 「看了很久」和「反复进出」。

use chrono::Local;
use sqlx::{Row, SqlitePool};

use crate::error::error::AppError;

#[derive(Clone)]
pub struct LiveStatsService {
    pool: SqlitePool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyLiveStats {
    pub date: String,
    pub seconds: i64,
    pub sessions: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRoomStats {
    pub platform: String,
    pub room_id: String,
    pub room_name: String,
    pub anchor_name: String,
    pub seconds: i64,
    pub sessions: i64,
    pub last_date: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveStatsSummary {
    pub total_seconds: i64,
    pub total_sessions: i64,
    pub total_rooms: i64,
    pub active_days: i64,
}

impl LiveStatsService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn today() -> String {
        Local::now().format("%Y-%m-%d").to_string()
    }

    /// 累加一次观看。`seconds` 是增量, `sessions` 通常为 0(仅在开始观看时传 1)。
    pub async fn add_watch(
        &self,
        user_ns: &str,
        seconds: i64,
        sessions: i64,
        date: Option<&str>,
        platform: &str,
        room_id: &str,
        room_name: &str,
        anchor_name: &str,
    ) -> Result<(), AppError> {
        let room_id = room_id.trim();
        if room_id.is_empty() {
            return Err(AppError::BadRequest("缺少直播间号".to_string()));
        }
        let date = date.map(String::from).unwrap_or_else(Self::today);
        sqlx::query(
            "INSERT INTO live_stats
               (user_ns, date, platform, room_id, room_name, anchor_name, seconds, sessions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(user_ns, date, platform, room_id) DO UPDATE SET
               room_name = CASE
                 WHEN excluded.room_name <> '' THEN excluded.room_name
                 ELSE live_stats.room_name
               END,
               anchor_name = CASE
                 WHEN excluded.anchor_name <> '' THEN excluded.anchor_name
                 ELSE live_stats.anchor_name
               END,
               seconds = seconds + excluded.seconds,
               sessions = sessions + excluded.sessions",
        )
        .bind(user_ns)
        .bind(&date)
        .bind(platform)
        .bind(room_id)
        .bind(room_name.trim())
        .bind(anchor_name.trim())
        .bind(seconds.max(0))
        .bind(sessions.max(0))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_daily(
        &self,
        user_ns: &str,
        start: &str,
        end: &str,
    ) -> Result<Vec<DailyLiveStats>, AppError> {
        let rows = sqlx::query(
            "SELECT date, SUM(seconds) AS seconds, SUM(sessions) AS sessions
             FROM live_stats
             WHERE user_ns = ?1 AND date >= ?2 AND date <= ?3
             GROUP BY date
             ORDER BY date ASC",
        )
        .bind(user_ns)
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|row| DailyLiveStats {
                date: row.get("date"),
                seconds: row.get("seconds"),
                sessions: row.get("sessions"),
            })
            .collect())
    }

    pub async fn get_summary(&self, user_ns: &str) -> Result<LiveStatsSummary, AppError> {
        let row = sqlx::query(
            "SELECT
               COALESCE(SUM(seconds), 0) AS total_seconds,
               COALESCE(SUM(sessions), 0) AS total_sessions,
               COUNT(DISTINCT platform || ':' || room_id) AS total_rooms,
               COUNT(DISTINCT date) AS active_days
             FROM live_stats WHERE user_ns = ?1",
        )
        .bind(user_ns)
        .fetch_one(&self.pool)
        .await?;
        Ok(LiveStatsSummary {
            total_seconds: row.get("total_seconds"),
            total_sessions: row.get("total_sessions"),
            total_rooms: row.get("total_rooms"),
            active_days: row.get("active_days"),
        })
    }

    pub async fn get_by_room(
        &self,
        user_ns: &str,
        start: &str,
        end: &str,
    ) -> Result<Vec<LiveRoomStats>, AppError> {
        let rows = sqlx::query(
            "SELECT platform, room_id,
                    MAX(room_name) AS room_name,
                    MAX(anchor_name) AS anchor_name,
                    SUM(seconds) AS seconds,
                    SUM(sessions) AS sessions,
                    MAX(date) AS last_date
             FROM live_stats
             WHERE user_ns = ?1 AND date >= ?2 AND date <= ?3
             GROUP BY platform, room_id
             ORDER BY seconds DESC",
        )
        .bind(user_ns)
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|row| LiveRoomStats {
                platform: row.get("platform"),
                room_id: row.get("room_id"),
                room_name: row.get("room_name"),
                anchor_name: row.get("anchor_name"),
                seconds: row.get("seconds"),
                sessions: row.get("sessions"),
                last_date: row.get("last_date"),
            })
            .collect())
    }

    /// 最近看过的直播间(按最后观看日期倒序), 即"历史"。
    /// 直接复用观看统计, 不再单独建历史表。
    pub async fn get_recent_rooms(
        &self,
        user_ns: &str,
        limit: u32,
    ) -> Result<Vec<LiveRoomStats>, AppError> {
        let rows = sqlx::query(
            "SELECT platform, room_id,
                    MAX(room_name) AS room_name,
                    MAX(anchor_name) AS anchor_name,
                    SUM(seconds) AS seconds,
                    SUM(sessions) AS sessions,
                    MAX(date) AS last_date
             FROM live_stats
             WHERE user_ns = ?1
             GROUP BY platform, room_id
             ORDER BY MAX(date) DESC, seconds DESC
             LIMIT ?2",
        )
        .bind(user_ns)
        .bind(limit.clamp(1, 200) as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|row| LiveRoomStats {
                platform: row.get("platform"),
                room_id: row.get("room_id"),
                room_name: row.get("room_name"),
                anchor_name: row.get("anchor_name"),
                seconds: row.get("seconds"),
                sessions: row.get("sessions"),
                last_date: row.get("last_date"),
            })
            .collect())
    }

    pub async fn delete_room(&self, user_ns: &str, platform: &str, room_id: &str) -> Result<u64, AppError> {
        let result = sqlx::query(
            "DELETE FROM live_stats WHERE user_ns = ?1 AND platform = ?2 AND room_id = ?3",
        )
        .bind(user_ns)
        .bind(platform)
        .bind(room_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn memory_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("open memory db");
        sqlx::migrate!("src/storage/db/migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        pool
    }

    #[tokio::test]
    async fn accumulates_seconds_and_sessions_per_room_and_day() {
        let service = LiveStatsService::new(memory_pool().await);
        service
            .add_watch("default", 60, 1, Some("2026-10-20"), "bilibili", "1", "房间", "主播")
            .await
            .expect("first watch");
        service
            .add_watch("default", 30, 0, Some("2026-10-20"), "bilibili", "1", "房间", "主播")
            .await
            .expect("second flush");

        let rooms = service
            .get_by_room("default", "2026-10-20", "2026-10-20")
            .await
            .expect("by room");
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].seconds, 90);
        assert_eq!(rooms[0].sessions, 1);
        assert_eq!(rooms[0].anchor_name, "主播");

        let daily = service
            .get_daily("default", "2026-10-20", "2026-10-20")
            .await
            .expect("daily");
        assert_eq!(daily.len(), 1);
        assert_eq!(daily[0].seconds, 90);

        let summary = service.get_summary("default").await.expect("summary");
        assert_eq!(summary.total_rooms, 1);
        assert_eq!(summary.active_days, 1);
    }

    #[tokio::test]
    async fn rejects_blank_room_id() {
        let service = LiveStatsService::new(memory_pool().await);
        assert!(service
            .add_watch("default", 1, 1, None, "bilibili", "  ", "", "")
            .await
            .is_err());
    }
}
