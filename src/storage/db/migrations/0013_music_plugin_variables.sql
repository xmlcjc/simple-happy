-- 音乐插件用户变量: 插件声明需要登录凭据等变量时, 由用户在插件设置里填写。
CREATE TABLE IF NOT EXISTS music_plugin_variable (
    platform TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (platform, key)
);
