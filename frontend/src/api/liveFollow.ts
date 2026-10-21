import { invokeEnvelope } from './invoke'
import type { LiveRoomStats } from './liveStats'

export interface LiveFollow {
  platform: string
  roomId: string
  roomName: string
  anchorName: string
  avatar: string
  createdAt: number
}

export function followLiveRoom(params: {
  platform: string
  roomId: string
  roomName?: string
  anchorName?: string
  avatar?: string
}): Promise<{ saved: boolean }> {
  return invokeEnvelope('live_follow_room', { req: params })
}

export function unfollowLiveRoom(platform: string, roomId: string): Promise<{ removed: number }> {
  return invokeEnvelope('live_unfollow_room', { platform, roomId })
}

export function getLiveFollows(): Promise<LiveFollow[]> {
  return invokeEnvelope('get_live_follows', {})
}

/** 观看历史复用观看统计的房间聚合，不单独建表。 */
export function getLiveHistory(limit = 50): Promise<LiveRoomStats[]> {
  return invokeEnvelope('get_live_history', { limit })
}
