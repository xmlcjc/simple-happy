//! 直播功能服务: 聚合各平台 provider, 对外暴露平台无关的统一接口。

pub mod bilibili;
pub mod danmaku;
pub mod douyin;
pub mod douyu;
pub mod follow;
pub mod huya;
pub mod models;
pub mod stats;

use crate::crawler::http_client::HttpClient;
use crate::error::error::AppError;
use models::{
    LiveCategoryGroup, LivePlatform, LiveRoomDetail, LiveRoomSummary, LiveStreamInfo,
};

pub struct LiveService {
    http: HttpClient,
    pub bilibili: bilibili::Bilibili,
    pub huya: huya::Huya,
    pub douyu: douyu::Douyu,
    pub douyin: douyin::Douyin,
}

impl LiveService {
    pub fn new(http: HttpClient) -> Self {
        Self {
            bilibili: bilibili::Bilibili::new(http.clone()),
            huya: huya::Huya::new(http.clone()),
            douyu: douyu::Douyu::new(http.clone()),
            douyin: douyin::Douyin::new(http.clone()),
            http,
        }
    }

    /// 弹幕模块复用同一套 HTTP 客户端(同一代理/超时配置)。
    pub fn http(&self) -> HttpClient {
        self.http.clone()
    }

    pub fn platforms(&self) -> Vec<LivePlatform> {
        vec![
            LivePlatform {
                id: bilibili::PLATFORM_ID.to_string(),
                name: bilibili::PLATFORM_NAME.to_string(),
            },
            LivePlatform {
                id: huya::PLATFORM_ID.to_string(),
                name: huya::PLATFORM_NAME.to_string(),
            },
            LivePlatform {
                id: douyu::PLATFORM_ID.to_string(),
                name: douyu::PLATFORM_NAME.to_string(),
            },
            LivePlatform {
                id: douyin::PLATFORM_ID.to_string(),
                name: douyin::PLATFORM_NAME.to_string(),
            },
        ]
    }

    pub async fn categories(&self, platform: &str) -> Result<Vec<LiveCategoryGroup>, AppError> {
        match platform {
            bilibili::PLATFORM_ID => self.bilibili.categories().await,
            huya::PLATFORM_ID => Ok(self.huya.categories().await),
            douyu::PLATFORM_ID => self.douyu.categories().await,
            douyin::PLATFORM_ID => self.douyin.categories().await,
            other => Err(unsupported_platform(other)),
        }
    }

    pub async fn rooms(
        &self,
        platform: &str,
        parent: &str,
        area: &str,
        page: u32,
    ) -> Result<Vec<LiveRoomSummary>, AppError> {
        match platform {
            bilibili::PLATFORM_ID => self.bilibili.rooms(parent, area, page).await,
            // 虎牙的房间列表按"具体游戏"(gid)拉取, 没有"整组全部"的语义:
            // 前端已保证 area 落到子分区; 万一没选, 退回一级分区 id(至少能出内容)。
            huya::PLATFORM_ID => {
                let game_id = if area.trim().is_empty() || area == "0" { parent } else { area };
                self.huya.rooms(game_id, page).await
            }
            // 斗鱼同样必须落到二级分区(cate2)。
            douyu::PLATFORM_ID => self.douyu.rooms(parent, area, page).await,
            douyin::PLATFORM_ID => self.douyin.rooms(parent, area, page).await,
            other => Err(unsupported_platform(other)),
        }
    }

    pub async fn search(
        &self,
        platform: &str,
        keyword: &str,
        page: u32,
    ) -> Result<Vec<LiveRoomSummary>, AppError> {
        match platform {
            bilibili::PLATFORM_ID => self.bilibili.search(keyword, page).await,
            huya::PLATFORM_ID => self.huya.search(keyword, page).await,
            douyu::PLATFORM_ID => self.douyu.search(keyword, page).await,
            douyin::PLATFORM_ID => self.douyin.search(keyword, page).await,
            other => Err(unsupported_platform(other)),
        }
    }

    pub async fn room_detail(
        &self,
        platform: &str,
        room_id: &str,
    ) -> Result<LiveRoomDetail, AppError> {
        match platform {
            bilibili::PLATFORM_ID => self.bilibili.room_detail(room_id).await,
            huya::PLATFORM_ID => self.huya.room_detail(room_id).await,
            douyu::PLATFORM_ID => self.douyu.room_detail(room_id).await,
            douyin::PLATFORM_ID => self.douyin.room_detail(room_id).await,
            other => Err(unsupported_platform(other)),
        }
    }

    pub async fn stream_urls(
        &self,
        platform: &str,
        room_id: &str,
    ) -> Result<LiveStreamInfo, AppError> {
        match platform {
            bilibili::PLATFORM_ID => self.bilibili.stream_urls(room_id).await,
            huya::PLATFORM_ID => self.huya.stream_urls(room_id).await,
            douyu::PLATFORM_ID => self.douyu.stream_urls(room_id).await,
            douyin::PLATFORM_ID => self.douyin.stream_urls(room_id).await,
            other => Err(unsupported_platform(other)),
        }
    }
}

fn unsupported_platform(platform: &str) -> AppError {
    AppError::BadRequest(format!("暂不支持的直播平台: {platform}"))
}
