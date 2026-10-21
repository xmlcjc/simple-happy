//! 音乐模块的对外数据模型。
//!
//! `MusicItem` 直接对齐 MusicFree 插件的 `IMusicItem`: 除常见字段外,
//! 插件常会附带自己的私有字段(例如音源 id、码率、歌词地址等)。
//! 这些字段必须原样保留并回传给 `getMediaSource` / `getLyric`,
//! 因此统一收进 `extra`(`#[serde(flatten)]`), 不做裁剪。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 音乐条目。字段与 MusicFree 插件协议一致, 额外字段无损保留。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicItem {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub artwork: String,
    /// 时长(秒)。MusicFree 用秒而非毫秒。
    #[serde(default)]
    pub duration: Option<f64>,
    /// 插件私有字段, 往返时必须保留。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl MusicItem {
    /// 写库前统一成 JSON; 读取时再反序列化, 保证插件字段不丢。
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// 一次搜索的结果页。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicSearchPage {
    pub items: Vec<MusicItem>,
    /// 插件是否已经没有下一页。
    pub is_end: bool,
    /// 实际执行搜索的插件名(聚合搜索时用于标注来源)。
    #[serde(default)]
    pub platform: String,
}

/// 播放地址。部分插件会要求 Referer/UA 才能取流。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicMediaSource {
    pub url: String,
    #[serde(default)]
    pub quality: String,
    #[serde(default)]
    pub headers: Map<String, Value>,
}

/// 歌词。`raw` 为 LRC 原文, `translation` 为可选翻译 LRC。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicLyric {
    #[serde(default)]
    pub raw: String,
    #[serde(default)]
    pub translation: String,
}

/// 已安装插件的元信息(不含脚本源码)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlugin {
    pub platform: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub src_url: String,
    pub file_path: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 从插件源码中解析出的元信息。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PluginManifest {
    pub platform: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub src_url: String,
    /// 插件声明支持的搜索类型, 例如 ["music", "album"]。
    pub supported_search_type: Vec<String>,
}
