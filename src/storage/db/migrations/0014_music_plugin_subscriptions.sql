-- 插件导入增强: 订阅源、排序与描述。
--
-- 订阅源保存 .js 直链或 .json 订阅清单(格式 {plugins:[{url}]}),
-- 对应 MusicFree 的 plugin.subscribeUrl 配置。

CREATE TABLE IF NOT EXISTS music_plugin_subscription (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_ns TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

ALTER TABLE music_plugin ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE music_plugin ADD COLUMN description TEXT NOT NULL DEFAULT '';
