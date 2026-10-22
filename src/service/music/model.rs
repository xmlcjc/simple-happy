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

/// 专辑 / 歌手 / 歌单这类「集合型」条目。
///
/// MusicFree 的歌手用 `name`/`avatar`, 专辑与歌单用 `title`/`artwork`;
/// 归一化后统一放进 title/artwork, 其余私有字段与 MusicItem 一样原样保留。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicCollectionItem {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub description: String,
    /// 收录曲目数(专辑/歌单)。
    #[serde(default)]
    pub works_num: Option<f64>,
    #[serde(default)]
    pub play_count: Option<f64>,
    #[serde(default)]
    pub date: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 集合型搜索的结果页。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicCollectionPage {
    pub items: Vec<MusicCollectionItem>,
    pub is_end: bool,
    pub platform: String,
    /// 本次搜索类型: album / artist / sheet。
    #[serde(default)]
    pub search_type: String,
}

/// 集合详情。专辑/歌单给 `items`; 歌手的「专辑」作品给 `collections`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicCollectionDetail {
    /// 插件回传的集合信息(可能补全了封面/简介), 没回传则为空。
    #[serde(default)]
    pub item: Option<MusicCollectionItem>,
    #[serde(default)]
    pub items: Vec<MusicItem>,
    #[serde(default)]
    pub collections: Vec<MusicCollectionItem>,
    #[serde(default)]
    pub is_end: bool,
}

/// 榜单分组：插件返回的榜单按组展示。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicSheetGroup {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub items: Vec<MusicCollectionItem>,
}

/// 推荐歌单的 tag。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicTag {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicTagGroup {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tags: Vec<MusicTag>,
}

/// 推荐歌单的 tag 列表：`pinned` 是插件固定的入口歌单。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicRecommendTags {
    #[serde(default)]
    pub pinned: Vec<MusicCollectionItem>,
    #[serde(default)]
    pub groups: Vec<MusicTagGroup>,
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
    /// 插件自述(支持 Markdown), 展示在插件列表的说明弹窗里。
    pub description: String,
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
    /// 插件要求的最低宿主版本(MusicFree 的 `appVersion` 语义, 支持 semver 区间)。
    pub app_version: String,
    pub description: String,
    /// 插件声明支持的搜索类型, 例如 ["music", "album"]。
    pub supported_search_type: Vec<String>,
}

/// 插件订阅源: `.js` 直链或 `.json` 订阅清单。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSubscription {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub created_at: i64,
}

/// 一次按 URL 安装的结果。失败不抛错, 由前端汇总展示。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInstallResult {
    pub url: String,
    pub success: bool,
    pub platform: String,
    pub name: String,
    pub version: String,
    pub error: String,
}

/// 插件声明需要用户填写的变量（登录 token、cookie 等）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginVariable {
    pub key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub hint: String,
}
