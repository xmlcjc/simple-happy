// 读取请求闸门：相同请求单飞合并 + 并发上限。
//
// 与 fqapp 的 RequestGate 同一思路（只做两件收敛的事）：
// - 合并：同 key 的请求在飞行中时，后来的调用直接复用同一个 Promise，
//   避免快速切页 / 双击 / 多个组件同时要同一章正文时重复打源站；
// - 限流：同时最多 maxConcurrent 个请求进入 Rust 命令层，其余 FIFO 排队。
//
// 只用于只读 GET（invoke.ts 的 get）。写操作仍逐个执行，绝不被合并。

interface Waiter {
  grant: () => void
}

export class RequestGate {
  private active = 0
  private readonly waiters: Waiter[] = []
  private readonly inflight = new Map<string, Promise<unknown>>()
  private readonly maxConcurrent: number

  constructor(maxConcurrent = 16) {
    this.maxConcurrent = maxConcurrent
  }

  run<T>(key: string, send: () => Promise<T>, share = true): Promise<T> {
    if (share) {
      const existing = this.inflight.get(key)
      if (existing) return existing as Promise<T>
    }

    const promise = this.acquire().then(send).finally(() => {
      this.release()
      if (share) this.inflight.delete(key)
    })

    if (share) this.inflight.set(key, promise as Promise<unknown>)
    return promise
  }

  private acquire(): Promise<void> {
    if (this.active < this.maxConcurrent) {
      this.active += 1
      return Promise.resolve()
    }
    return new Promise<void>((resolve) => {
      this.waiters.push({ grant: resolve })
    })
  }

  private release() {
    const waiter = this.waiters.shift()
    if (waiter) {
      // 名额直接转交，active 保持不变。
      waiter.grant()
    } else {
      this.active -= 1
    }
  }
}

/** 对参数做稳定序列化：键排序后再拼接，避免字段顺序不同导致无法合并。 */
export function stableKey(value: unknown): string {
  if (value === undefined) return ''
  if (value === null || typeof value !== 'object') return JSON.stringify(value)
  if (Array.isArray(value)) return `[${value.map(stableKey).join(',')}]`
  const record = value as Record<string, unknown>
  return `{${Object.keys(record)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${stableKey(record[key])}`)
    .join(',')}}`
}

export const requestGate = new RequestGate(16)
