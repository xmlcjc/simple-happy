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
    /// 开播时间(unix 秒); 0 表示平台未提供(前端不显示开播时长)。
    #[serde(default)]
    pub live_start_time: i64,
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
    /// 平台认可的全部清晰度(按从高到低排列)。
    ///
    /// B 站要单独取一次 `accept_qn` 才能拿到完整列表, 且列表随登录状态变化;
    /// 其它平台按实际返回的线路归并, 这里留空由前端回退。
    #[serde(default)]
    pub qualities: Vec<LiveQuality>,
}

/// 一个可选清晰度。`qn` 是平台内部的清晰度编号, 取流时原样回传。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveQuality {
    pub qn: i64,
    pub name: String,
}

/// B 站扫码登录的二维码内容与轮询 key。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveQrCode {
    /// 二维码里编码的登录地址, 前端渲染成图片。
    pub url: String,
    pub qrcode_key: String,
}

/// 扫码轮询结果。`status`: pending / scanned / expired / success。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveQrCodeStatus {
    pub status: String,
    #[serde(default)]
    pub message: String,
    /// 仅 `success` 时有值: 可直接写入直播设置的 Cookie。
    #[serde(default)]
    pub cookie: String,
}

/// 一条 SuperChat(醒目留言), 由弹幕实时推送 + 进房时拉取历史合并。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSuperChat {
    pub user: String,
    pub avatar: String,
    pub message: String,
    /// 金额(元)。
    pub price: i64,
    /// 生效时刻(unix 秒)。
    pub start_time: i64,
    /// 失效时刻(unix 秒)。
    pub end_time: i64,
}
