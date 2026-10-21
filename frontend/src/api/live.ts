import { invokeEnvelope } from './invoke'

export interface LivePlatform {
  id: string
  name: string
}

export interface LiveCategory {
  id: string
  name: string
  /** 分区图标；平台没给就是空串。 */
  icon?: string
}

export interface LiveCategoryGroup {
  id: string
  name: string
  children: LiveCategory[]
}

export interface LiveRoomSummary {
  platform: string
  roomId: string
  title: string
  cover: string
  userName: string
  online: number
  areaName: string
}

export interface LiveRoomDetail {
  platform: string
  roomId: string
  title: string
  cover: string
  userName: string
  avatar: string
  online: number
  followers: number
  description: string
  areaName: string
  parentAreaName: string
  isLive: boolean
}

export interface LiveStream {
  /** `hls` 可被 WebView 直接播放；`flv` 需要 Referer，浏览器内不可用。 */
  format: string
  quality: string
  url: string
}

export interface LiveStreamInfo {
  roomId: string
  isLive: boolean
  streams: LiveStream[]
}

export function getLivePlatforms(): Promise<LivePlatform[]> {
  return invokeEnvelope('live_platforms', {})
}

export function getLiveCategories(platform: string): Promise<LiveCategoryGroup[]> {
  return invokeEnvelope('live_categories', { platform })
}

export function getLiveRooms(params: {
  platform: string
  parentAreaId: string
  areaId?: string
  page?: number
}): Promise<LiveRoomSummary[]> {
  return invokeEnvelope('live_rooms', {
    platform: params.platform,
    parentAreaId: params.parentAreaId,
    areaId: params.areaId ?? '0',
    page: params.page ?? 1,
  })
}

export function getLiveRoomDetail(platform: string, roomId: string): Promise<LiveRoomDetail> {
  return invokeEnvelope('live_room_detail', { platform, roomId })
}

export function searchLiveRooms(params: {
  platform: string
  keyword: string
  page?: number
}): Promise<LiveRoomSummary[]> {
  return invokeEnvelope('live_search', {
    platform: params.platform,
    keyword: params.keyword,
    page: params.page ?? 1,
  })
}

export function getLiveStreamUrls(platform: string, roomId: string): Promise<LiveStreamInfo> {
  return invokeEnvelope('live_stream_urls', { platform, roomId })
}
