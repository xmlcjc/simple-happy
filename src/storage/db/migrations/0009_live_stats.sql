CREATE TABLE IF NOT EXISTS live_stats (
    user_ns TEXT NOT NULL,
    date TEXT NOT NULL,
    platform TEXT NOT NULL,
    room_id TEXT NOT NULL,
    room_name TEXT NOT NULL DEFAULT '',
    anchor_name TEXT NOT NULL DEFAULT '',
    seconds INTEGER NOT NULL DEFAULT 0,
    sessions INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_ns, date, platform, room_id)
);

CREATE INDEX IF NOT EXISTS idx_live_stats_user_ns_date
ON live_stats(user_ns, date);
