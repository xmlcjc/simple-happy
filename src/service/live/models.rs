//! 直播模块的跨端数据模型。
//!
//! 字段一律 `camelCase`(前端直接用), 平台差异在这一层抹平: 新增平台只需实现
//! 一个 provider, 前端视图不用改。

use serde::{Deserialize, Serialize};

/// 支持的直播平台。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePlatform {
    pub id: String,
    pub name: String,
}

/// 二级分区。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveCategory {
    pub id: String,
    pub name: String,
    /// 分区图标(圆形/方形网格用); 平台没给就是空串。
    #[serde(default)]
    pub icon: String,
}

/// 一级分区(带二级子分区)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveCategoryGroup {
    pub id: String,
    pub name: String,
    pub children: Vec<LiveCategory>,
}

/// 房间列表项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRoomSummary {
    pub platform: String,
    pub room_id: String,
    pub title: String,
    pub cover: String,
    pub user_name: String,
    pub online: u64,
    pub area_name: String,
}

/// 直播间详情。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRoomDetail {
    pub platform: String,
    pub room_id: String,
    pub title: String,
    pub cover: String,
    pub user_name: String,
    pub avatar: String,
    pub online: u64,
    pub followers: u64,
    pub description: String,
    pub area_name: String,
    pub parent_area_name: String,
    pub is_live: bool,
}

/// 一条可直接播放的流地址。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveStream {
    /// `hls` 或 `flv`。
    pub format: String,
    pub quality: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveStreamInfo {
    pub room_id: String,
    pub is_live: bool,
    pub streams: Vec<LiveStream>,
}
