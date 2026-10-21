import { invoke, Channel } from '@tauri-apps/api/core'
import type { ApiResponse } from '../types'
import { toError } from './invoke'

export interface DanmakuMessage {
  kind: 'chat' | 'gift' | 'enter' | 'superchat' | 'error'
  user: string
  content: string
  /** `#rrggbb`；空串表示用主题色。 */
  color: string
  timestamp: number
  /** SC 金额（元）；非 SC 消息为 0。 */
  price?: number
  /** SC 展示时长（秒）；非 SC 消息为 0。 */
  duration?: number
  /** SC 发送者头像；非 SC 消息为空串。 */
  avatar?: string
}

/**
 * 开始接收弹幕。后端维持 WebSocket 并批量推送，前端只渲染。
 * 返回后不要重复调用，同一房间重复 start 会顶掉上一条连接。
 */
export async function startLiveDanmaku(
  platform: string,
  roomId: string,
  onBatch: (messages: DanmakuMessage[]) => void,
): Promise<void> {
  const channel = new Channel<DanmakuMessage[]>()
  channel.onmessage = (payload) => {
    if (Array.isArray(payload) && payload.length) onBatch(payload)
  }
  const res = await invoke<ApiResponse<unknown>>('live_danmaku_start', {
    platform,
    roomId,
    onEvent: channel,
  }).catch((err) => {
    throw toError(err)
  })
  if (!res.isSuccess) throw new Error(res.errorMsg || '启动弹幕失败')
}

export async function stopLiveDanmaku(platform: string, roomId: string): Promise<void> {
  const res = await invoke<ApiResponse<unknown>>('live_danmaku_stop', { platform, roomId })
  if (!res.isSuccess) throw new Error(res.errorMsg || '停止弹幕失败')
}
