import { invokeEnvelope } from './invoke'

export interface LogSnapshot {
  path: string
  lines: string[]
}

/** 读取最近日志行（与 Rust tracing 写的是同一份文件）。 */
export function readLogs(limit = 1000): Promise<LogSnapshot> {
  return invokeEnvelope<LogSnapshot>('log_read', { limit })
}

/** 前端写入一行日志；失败静默，日志不能影响主链路。 */
export function appendLog(level: 'debug' | 'info' | 'warn' | 'error', tag: string, message: string) {
  return invokeEnvelope<{ written: boolean }>('log_append', {
    req: { level, tag, message },
  }).catch(() => ({ written: false }))
}

/** 清空当前日志与轮换文件。 */
export function clearLogs(): Promise<{ cleared: boolean }> {
  return invokeEnvelope('log_clear', {})
}

/** 常用的一次性事件记录；不阻塞调用方。 */
export function logEvent(
  level: 'debug' | 'info' | 'warn' | 'error',
  tag: string,
  message: string,
) {
  void appendLog(level, tag, message)
}
