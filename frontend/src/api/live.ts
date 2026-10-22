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
  /** 开播时间（unix 秒）；0 表示平台未提供，不显示开播时长。 */
  liveStartTime: number
}

/** 一条 SuperChat（醒目留言），进房时拉取展示期内的列表。 */
export interface LiveSuperChat {
  user: string
  avatar: string
  message: string
  /** 金额（元）。 */
  price: number
  /** 生效时刻（unix 秒）。 */
  startTime: number
  /** 失效时刻（unix 秒）。 */
  endTime: number
}

export interface LiveStream {
  /** `hls` 可被 WebView 直接播放；`flv` 需要 Referer，浏览器内不可用。 */
  format: string
  quality: string
  url: string
}

/** 一个可选清晰度。`qn` 是平台内部编号，切换时原样回传给后端。 */
export interface LiveQuality {
  qn: number
  name: string
}

export interface LiveStreamInfo {
  roomId: string
  isLive: boolean
  streams: LiveStream[]
  /** 平台认可的全部清晰度（B 站登录后档位更多）；其它平台为空。 */
  qualities?: LiveQuality[]
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

export function getLiveStreamUrls(
  platform: string,
  roomId: string,
  options: { cookie?: string; qn?: number } = {},
): Promise<LiveStreamInfo> {
  return invokeEnvelope('live_stream_urls', {
    platform,
    roomId,
    cookie: options.cookie || undefined,
    qn: options.qn,
  })
}

/** B 站扫码登录二维码：`url` 由前端渲染成二维码图片。 */
export interface LiveQrCode {
  url: string
  qrcodeKey: string
}

/** 扫码轮询状态：pending / scanned / expired / success。 */
export interface LiveQrCodeStatus {
  status: string
  message: string
  /** 仅 success 时有值，可直接写入直播设置的 Cookie。 */
  cookie: string
}

export function getBilibiliQrCode(): Promise<LiveQrCode> {
  return invokeEnvelope('live_bilibili_qrcode', {})
}

export function pollBilibiliQrCode(qrcodeKey: string): Promise<LiveQrCodeStatus> {
  return invokeEnvelope('live_bilibili_qrcode_poll', { qrcodeKey })
}

/** 房间内仍在展示期内的 SC 列表（目前仅 B 站支持，其它平台返回空）。 */
export function getLiveSuperChatHistory(platform: string, roomId: string): Promise<LiveSuperChat[]> {
  return invokeEnvelope('get_live_superchat_history', { platform, roomId })
}
