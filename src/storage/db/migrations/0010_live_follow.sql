CREATE TABLE IF NOT EXISTS live_follow (
    user_ns TEXT NOT NULL,
    platform TEXT NOT NULL,
    room_id TEXT NOT NULL,
    room_name TEXT NOT NULL DEFAULT '',
    anchor_name TEXT NOT NULL DEFAULT '',
    avatar TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_ns, platform, room_id)
);

CREATE INDEX IF NOT EXISTS idx_live_follow_user_ns
ON live_follow(user_ns, created_at DESC);
