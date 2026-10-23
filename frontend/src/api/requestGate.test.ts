import { describe, expect, it } from 'vitest'
import { RequestGate, stableKey } from './requestGate'

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

describe('RequestGate', () => {
  it('coalesces identical in-flight requests', async () => {
    const gate = new RequestGate(4)
    const pending = deferred<string>()
    let calls = 0
    const send = () => {
      calls += 1
      return pending.promise
    }

    const first = gate.run('same', send)
    const second = gate.run('same', send)
    await Promise.resolve()
    expect(calls).toBe(1)

    pending.resolve('ok')
    await expect(Promise.all([first, second])).resolves.toEqual(['ok', 'ok'])
  })

  it('runs different keys independently and applies the concurrency cap', async () => {
    const gate = new RequestGate(2)
    let active = 0
    let peak = 0
    const releases: Array<() => void> = []

    const send = () =>
      new Promise<number>((resolve) => {
        active += 1
        peak = Math.max(peak, active)
        releases.push(() => {
          active -= 1
          resolve(active)
        })
      })

    const tasks = ['a', 'b', 'c', 'd'].map((key) => gate.run(key, send))
    await Promise.resolve()
    // 只有前两个进入发送，后两个在排队。
    expect(releases).toHaveLength(2)

    while (releases.length) {
      releases.shift()?.()
      await Promise.resolve()
      await Promise.resolve()
    }

    await Promise.all(tasks)
    expect(peak).toBeLessThanOrEqual(2)
  })

  it('stableKey is order-insensitive for object keys', () => {
    expect(stableKey({ a: 1, b: [2, 3] })).toBe(stableKey({ b: [2, 3], a: 1 }))
    expect(stableKey(undefined)).toBe('')
  })
})
