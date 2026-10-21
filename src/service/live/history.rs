//! 直播观看历史。
//!
//! 进房即记录(与 simple_live 的 addHistory 一致), 不依赖观看统计:
//! 统计按天聚合且只有出画面才上报, 用它当历史会漏掉「进过但没播成」的房间。
//! 名字/封面/头像在访问时快照, 列表页不用再打平台接口。

use sqlx::{Row, SqlitePool};

use crate::error::error::AppError;

#[derive(Clone)]
pub struct LiveHistoryService {
    pool: SqlitePool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveHistoryEntry {
    pub platform: String,
    pub room_id: String,
    pub room_name: String,
    pub anchor_name: String,
    pub cover: String,
    pub avatar: String,
    /// 最近一次进入房间的时刻(unix 毫秒)。
    pub visited_at: i64,
}

impl LiveHistoryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 记录一次进房; 重复进同一房间只更新快照与时间(PK 决定唯一)。
    #[allow(clippy::too_many_arguments)]
    pub async fn add(
        &self,
        user_ns: &str,
        platform: &str,
        room_id: &str,
        room_name: &str,
        anchor_name: &str,
        cover: &str,
        avatar: &str,
    ) -> Result<(), AppError> {
        let room_id = room_id.trim();
        if room_id.is_empty() {
            return Err(AppError::BadRequest("缺少直播间号".to_string()));
        }
        sqlx::query(
            "INSERT INTO live_history
               (user_ns, platform, room_id, room_name, anchor_name, cover, avatar, visited_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
               MAX(?8, COALESCE((SELECT MAX(visited_at) + 1 FROM live_history WHERE user_ns = ?1), ?8)))
             ON CONFLICT(user_ns, platform, room_id) DO UPDATE SET
               room_name = CASE WHEN excluded.room_name <> '' THEN excluded.room_name ELSE live_history.room_name END,
               anchor_name = CASE WHEN excluded.anchor_name <> '' THEN excluded.anchor_name ELSE live_history.anchor_name END,
               cover = CASE WHEN excluded.cover <> '' THEN excluded.cover ELSE live_history.cover END,
               avatar = CASE WHEN excluded.avatar <> '' THEN excluded.avatar ELSE live_history.avatar END,
               visited_at = excluded.visited_at",
        )
        .bind(user_ns)
        .bind(platform)
        .bind(room_id)
        .bind(room_name.trim())
        .bind(anchor_name.trim())
        .bind(cover.trim())
        .bind(avatar.trim())
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 最近看过的直播间(按访问时间倒序)。
    pub async fn list(&self, user_ns: &str, limit: u32) -> Result<Vec<LiveHistoryEntry>, AppError> {
        let rows = sqlx::query(
            "SELECT platform, room_id, room_name, anchor_name, cover, avatar, visited_at
             FROM live_history WHERE user_ns = ?1
             ORDER BY visited_at DESC, rowid DESC
             LIMIT ?2",
        )
        .bind(user_ns)
        .bind(limit.clamp(1, 200) as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|row| LiveHistoryEntry {
                platform: row.get("platform"),
                room_id: row.get("room_id"),
                room_name: row.get("room_name"),
                anchor_name: row.get("anchor_name"),
                cover: row.get("cover"),
                avatar: row.get("avatar"),
                visited_at: row.get("visited_at"),
            })
            .collect())
    }

    /// 删除单条历史。
    pub async fn remove(
        &self,
        user_ns: &str,
        platform: &str,
        room_id: &str,
    ) -> Result<u64, AppError> {
        let result = sqlx::query(
            "DELETE FROM live_history WHERE user_ns = ?1 AND platform = ?2 AND room_id = ?3",
        )
        .bind(user_ns)
        .bind(platform)
        .bind(room_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// 清空全部历史。
    pub async fn clear(&self, user_ns: &str) -> Result<u64, AppError> {
        let result = sqlx::query("DELETE FROM live_history WHERE user_ns = ?1")
            .bind(user_ns)
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
    async fn records_visits_in_reverse_order_with_snapshots() {
        let service = LiveHistoryService::new(memory_pool().await);
        service
            .add("default", "bilibili", "1", "旧标题", "主播", "https://c.jpg", "https://a.jpg")
            .await
            .expect("first visit");
        // 第二次进房: 只更新非空快照, visited_at 顶到最前。
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        service
            .add("default", "bilibili", "1", "新标题", "", "", "")
            .await
            .expect("second visit");
        service
            .add("default", "douyu", "2", "斗鱼房", "斗鱼主播", "", "")
            .await
            .expect("other platform");

        let list = service.list("default", 50).await.expect("list");
        assert_eq!(list.len(), 2);
        // 最近访问的排最前: douyu:2 的 visited_at 更大。
        assert_eq!(list[0].platform, "douyu");
        assert_eq!(list[1].room_name, "新标题");
        // 空快照不覆盖旧值。
        assert_eq!(list[1].anchor_name, "主播");
        assert_eq!(list[1].cover, "https://c.jpg");
    }

    #[tokio::test]
    async fn orders_visits_within_same_millisecond() {
        let service = LiveHistoryService::new(memory_pool().await);
        // 不 sleep：Windows 时钟粒度可能是 15ms，同一毫秒内的连续进房也必须严格有序。
        for (platform, room) in [("bilibili", "1"), ("douyu", "2"), ("huya", "3")] {
            service
                .add("default", platform, room, room, "", "", "")
                .await
                .expect("visit");
        }
        let list = service.list("default", 50).await.expect("list");
        let order: Vec<&str> = list.iter().map(|item| item.platform.as_str()).collect();
        assert_eq!(order, vec!["huya", "douyu", "bilibili"]);
    }

    #[tokio::test]
    async fn removes_and_clears() {
        let service = LiveHistoryService::new(memory_pool().await);
        service
            .add("default", "bilibili", "1", "房", "主", "", "")
            .await
            .expect("visit");
        service
            .add("default", "douyu", "2", "房2", "主2", "", "")
            .await
            .expect("visit2");

        let removed = service.remove("default", "bilibili", "1").await.expect("remove");
        assert_eq!(removed, 1);
        assert_eq!(service.list("default", 50).await.expect("list").len(), 1);

        let cleared = service.clear("default").await.expect("clear");
        assert_eq!(cleared, 1);
        assert!(service.list("default", 50).await.expect("list").is_empty());
    }

    #[tokio::test]
    async fn rejects_blank_room_id() {
        let service = LiveHistoryService::new(memory_pool().await);
        assert!(service
            .add("default", "bilibili", " ", "", "", "", "")
            .await
            .is_err());
    }
}
