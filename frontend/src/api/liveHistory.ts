import { invokeEnvelope } from './invoke'

/** 观看历史条目：进房即记录，名字/封面/头像在访问时快照。 */
export interface LiveHistoryEntry {
  platform: string
  roomId: string
  roomName: string
  anchorName: string
  cover: string
  avatar: string
  /** 最近一次进入房间的时刻（unix 毫秒）。 */
  visitedAt: number
}

export function getLiveHistory(limit = 50): Promise<LiveHistoryEntry[]> {
  return invokeEnvelope('get_live_history', { limit })
}

export function addLiveHistory(params: {
  platform: string
  roomId: string
  roomName?: string
  anchorName?: string
  cover?: string
  avatar?: string
}): Promise<{ saved: boolean }> {
  return invokeEnvelope('live_add_history', { req: params })
}

export function deleteLiveHistoryRoom(
  platform: string,
  roomId: string,
): Promise<{ deleted: number }> {
  return invokeEnvelope('delete_live_history_room', { platform, roomId })
}

export function clearLiveHistory(): Promise<{ deleted: number }> {
  return invokeEnvelope('clear_live_history', {})
}
