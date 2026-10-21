-- 直播观看历史: 进房即记录(不依赖播放统计), 保存封面/头像快照与精确访问时间。
CREATE TABLE IF NOT EXISTS live_history (
    user_ns TEXT NOT NULL,
    platform TEXT NOT NULL,
    room_id TEXT NOT NULL,
    room_name TEXT NOT NULL DEFAULT '',
    anchor_name TEXT NOT NULL DEFAULT '',
    cover TEXT NOT NULL DEFAULT '',
    avatar TEXT NOT NULL DEFAULT '',
    visited_at INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_ns, platform, room_id)
);

CREATE INDEX IF NOT EXISTS idx_live_history_visited
    ON live_history (user_ns, visited_at DESC);
