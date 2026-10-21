-- 音乐功能基础表。所有用户数据沿用 live_follow 的 user_ns 维度,
-- 当前单用户场景固定为 'default'。

CREATE TABLE IF NOT EXISTS music_plugin (
    platform TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    version TEXT NOT NULL DEFAULT '',
    author TEXT NOT NULL DEFAULT '',
    src_url TEXT NOT NULL DEFAULT '',
    file_path TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS music_favorite (
    user_ns TEXT NOT NULL,
    platform TEXT NOT NULL,
    item_id TEXT NOT NULL,
    item_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_ns, platform, item_id)
);

CREATE TABLE IF NOT EXISTS music_playlist (
    user_ns TEXT NOT NULL,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_ns, id)
);

CREATE TABLE IF NOT EXISTS music_playlist_item (
    user_ns TEXT NOT NULL,
    playlist_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    item_id TEXT NOT NULL,
    item_json TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (user_ns, playlist_id, platform, item_id)
);

CREATE INDEX IF NOT EXISTS idx_music_playlist_item_order
ON music_playlist_item(user_ns, playlist_id, sort_order);

CREATE TABLE IF NOT EXISTS music_history (
    user_ns TEXT NOT NULL,
    platform TEXT NOT NULL,
    item_id TEXT NOT NULL,
    item_json TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    played_seconds INTEGER NOT NULL DEFAULT 0,
    finished INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_ns, platform, item_id)
);

CREATE INDEX IF NOT EXISTS idx_music_history_time
ON music_history(user_ns, started_at DESC);

CREATE TABLE IF NOT EXISTS music_daily_stat (
    user_ns TEXT NOT NULL,
    day TEXT NOT NULL,
    seconds INTEGER NOT NULL DEFAULT 0,
    plays INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_ns, day)
);

CREATE TABLE IF NOT EXISTS music_local_track (
    user_ns TEXT NOT NULL,
    path TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    artist TEXT NOT NULL DEFAULT '',
    album TEXT NOT NULL DEFAULT '',
    duration_ms INTEGER NOT NULL DEFAULT 0,
    format TEXT NOT NULL DEFAULT '',
    size INTEGER NOT NULL DEFAULT 0,
    mtime INTEGER NOT NULL DEFAULT 0,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (user_ns, path)
);
