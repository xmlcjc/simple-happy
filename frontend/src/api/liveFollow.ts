import { invokeEnvelope } from './invoke'

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
