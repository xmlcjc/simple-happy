// 加载诊断采样：把一次「打开正文 / 进入直播间」拆成固定阶段，只记录
// 白名单里的标签、耗时与数值错误码，最后一行 JSON 写入应用日志。
//
// 与 fqapp 的 PlayerLoadSample 同一纪律：绝不记录 URL、Cookie、密钥、
// 请求体与响应体，错误只保留阶段标签 + 可选的 HTTP 状态/错误码。
// 诊断失败不能影响主链路，所有写入都是 fire-and-forget。

import { logEvent } from '../api/log'

export type TraceMeta = Record<string, string | number | boolean | null | undefined>

export interface LoadTrace {
  /** 记录一个阶段耗时（同名阶段只记第一次，重复回调不会覆盖）。 */
  stage: (name: string) => void
  /** 成功结束；outcome 用固定标签（firstFrame / success / offline 等）。 */
  finish: (outcome?: string, extra?: TraceMeta) => void
  /** 失败结束：failureStage + 数值错误码/HTTP 状态。 */
  fail: (stage: string, extra?: TraceMeta) => void
}

/** 认可的阶段/结果标签：调用方传错只会落到 other，不会把任意字符串写进日志。 */
const ALLOWED_STAGES = new Set([
  'detail',
  'stream',
  'quality',
  'player',
  'firstFrame',
  'fetching',
  'parsing',
  'caching',
  'switchLine',
  'retry',
])

function safeLabel(value: string, allowed: Set<string>) {
  return allowed.has(value) ? value : 'other'
}

function safeMeta(meta: TraceMeta): Record<string, string | number | boolean | null> {
  const out: Record<string, string | number | boolean | null> = {}
  for (const [key, value] of Object.entries(meta)) {
    if (value === undefined) continue
    if (typeof value === 'number') {
      // 只保留有限数值，避免 NaN/Infinity 进入日志。
      if (Number.isFinite(value)) out[key] = Math.round(value)
    } else if (typeof value === 'string') {
      // 标签类字段只允许短 ASCII 单词，杜绝把 URL/消息正文带进来。
      if (/^[A-Za-z0-9._-]{1,24}$/.test(value)) out[key] = value
    } else if (typeof value === 'boolean' || value === null) {
      out[key] = value
    }
  }
  return out
}

export function createLoadTrace(tag: string, meta: TraceMeta = {}): LoadTrace {
  const started = typeof performance !== 'undefined' ? performance.now() : Date.now()
  let last = started
  let finished = false
  const stages: Record<string, number> = {}
  const base = safeMeta(meta)

  const emit = (outcome: string, extra: TraceMeta) => {
    if (finished) return
    finished = true
    const now = typeof performance !== 'undefined' ? performance.now() : Date.now()
    const payload = {
      ...base,
      ...safeMeta(extra),
      stages,
      totalMs: Math.round(now - started),
      outcome: safeLabel(outcome, new Set([...ALLOWED_STAGES, 'success', 'firstFrame', 'offline', 'error'])),
    }
    logEvent(outcome === 'error' ? 'warn' : 'info', tag, JSON.stringify(payload))
  }

  return {
    stage(name: string) {
      if (finished) return
      const now = typeof performance !== 'undefined' ? performance.now() : Date.now()
      const label = safeLabel(name, ALLOWED_STAGES)
      if (label !== 'other' && stages[label] === undefined) {
        stages[label] = Math.round(now - last)
      }
      last = now
    },
    finish(outcome = 'success', extra = {}) {
      emit(outcome, extra)
    },
    fail(stage, extra = {}) {
      emit('error', { failureStage: safeLabel(stage, ALLOWED_STAGES), ...extra })
    },
  }
}
