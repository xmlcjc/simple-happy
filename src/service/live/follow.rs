//! 直播关注列表。
//!
//! 纯本地数据(不需要平台账号): 关注关系落在 sqlite, 与书籍/书源同库同用户命名空间。
//! 房间名/主播名/头像在关注时快照一份, 避免列表页为了显示名字去打 N 个平台接口。

use sqlx::{Row, SqlitePool};

use crate::error::error::AppError;

#[derive(Clone)]
pub struct LiveFollowService {
    pool: SqlitePool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveFollow {
    pub platform: String,
    pub room_id: String,
    pub room_name: String,
    pub anchor_name: String,
    pub avatar: String,
    pub created_at: i64,
}

impl LiveFollowService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn follow(
        &self,
        user_ns: &str,
        platform: &str,
        room_id: &str,
        room_name: &str,
        anchor_name: &str,
        avatar: &str,
    ) -> Result<(), AppError> {
        let room_id = room_id.trim();
        if room_id.is_empty() {
            return Err(AppError::BadRequest("缺少直播间号".to_string()));
        }
        sqlx::query(
            "INSERT INTO live_follow
               (user_ns, platform, room_id, room_name, anchor_name, avatar, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(user_ns, platform, room_id) DO UPDATE SET
               room_name = CASE WHEN excluded.room_name <> '' THEN excluded.room_name ELSE live_follow.room_name END,
               anchor_name = CASE WHEN excluded.anchor_name <> '' THEN excluded.anchor_name ELSE live_follow.anchor_name END,
               avatar = CASE WHEN excluded.avatar <> '' THEN excluded.avatar ELSE live_follow.avatar END",
        )
        .bind(user_ns)
        .bind(platform)
        .bind(room_id)
        .bind(room_name.trim())
        .bind(anchor_name.trim())
        .bind(avatar.trim())
        // 重复关注保持原有关注时间, 让列表顺序稳定。
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn unfollow(&self, user_ns: &str, platform: &str, room_id: &str) -> Result<u64, AppError> {
        let result = sqlx::query(
            "DELETE FROM live_follow WHERE user_ns = ?1 AND platform = ?2 AND room_id = ?3",
        )
        .bind(user_ns)
        .bind(platform)
        .bind(room_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn list(&self, user_ns: &str) -> Result<Vec<LiveFollow>, AppError> {
        let rows = sqlx::query(
            "SELECT platform, room_id, room_name, anchor_name, avatar, created_at
             FROM live_follow WHERE user_ns = ?1
             ORDER BY created_at DESC",
        )
        .bind(user_ns)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|row| LiveFollow {
                platform: row.get("platform"),
                room_id: row.get("room_id"),
                room_name: row.get("room_name"),
                anchor_name: row.get("anchor_name"),
                avatar: row.get("avatar"),
                created_at: row.get("created_at"),
            })
            .collect())
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
    async fn follows_are_idempotent_and_removable() {
        let service = LiveFollowService::new(memory_pool().await);
        service
            .follow("default", "bilibili", "1", "房间", "主播", "https://a.jpg")
            .await
            .expect("follow");
        service
            .follow("default", "bilibili", "1", "新名字", "新主播", "")
            .await
            .expect("re-follow");
        let list = service.list("default").await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].room_name, "新名字");
        assert_eq!(list[0].avatar, "https://a.jpg");

        let removed = service
            .unfollow("default", "bilibili", "1")
            .await
            .expect("unfollow");
        assert_eq!(removed, 1);
        assert!(service.list("default").await.expect("list").is_empty());
    }

    #[tokio::test]
    async fn rejects_blank_room_id() {
        let service = LiveFollowService::new(memory_pool().await);
        assert!(service
            .follow("default", "bilibili", " ", "", "", "")
            .await
            .is_err());
    }
}
