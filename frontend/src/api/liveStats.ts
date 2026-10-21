import { invokeEnvelope } from './invoke'

export interface DailyLiveStats {
  date: string
  seconds: number
  sessions: number
}

export interface LiveRoomStats {
  platform: string
  roomId: string
  roomName: string
  anchorName: string
  seconds: number
  sessions: number
  lastDate: string
}

export interface LiveStatsSummary {
  totalSeconds: number
  totalSessions: number
  totalRooms: number
  activeDays: number
}

export function addLiveStats(params: {
  seconds?: number
  sessions?: number
  date?: string
  platform: string
  roomId: string
  roomName?: string
  anchorName?: string
}): Promise<{ saved: boolean }> {
  return invokeEnvelope('add_live_stats', { req: params })
}

export function getLiveStatsDaily(start: string, end: string): Promise<DailyLiveStats[]> {
  return invokeEnvelope('get_live_stats_daily', { start, end })
}

export function getLiveStatsSummary(): Promise<LiveStatsSummary> {
  return invokeEnvelope('get_live_stats_summary', {})
}

export function getLiveStatsByRoom(start: string, end: string): Promise<LiveRoomStats[]> {
  return invokeEnvelope('get_live_stats_by_room', { start, end })
}

export function deleteLiveStatsByRoom(platform: string, roomId: string): Promise<{ deleted: number }> {
  return invokeEnvelope('delete_live_stats_by_room', { platform, roomId })
}
