import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useLiveSettingsStore } from './liveSettings'

function installLocalStorage(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial))
  vi.stubGlobal('localStorage', {
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => values.set(key, value)),
    removeItem: vi.fn((key: string) => values.delete(key)),
  })
  return values
}

describe('live settings store', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    setActivePinia(createPinia())
  })

  it('uses sensible defaults and persists every change', () => {
    const values = installLocalStorage()
    const store = useLiveSettingsStore()

    expect(store.danmakuOn).toBe(true)
    expect(store.danmakuArea).toBe(50)
    expect(store.danmakuOpacity).toBe(1)
    expect(store.danmakuFontSize).toBe(16)
    expect(store.danmakuSpeed).toBe(9)

    store.setDanmakuOn(false)
    store.setDanmakuArea(75)
    store.setDanmakuOpacity(0.6)
    store.setDanmakuFontSize(20)
    store.setDanmakuSpeed(12)

    expect(values.get('live-danmaku-on')).toBe('false')
    expect(values.get('live-danmaku-area')).toBe('75')
    expect(values.get('live-danmaku-opacity')).toBe('0.6')
    expect(values.get('live-danmaku-font-size')).toBe('20')
    expect(values.get('live-danmaku-speed')).toBe('12')
  })

  it('restores saved preferences', () => {
    installLocalStorage({
      'live-danmaku-on': 'false',
      'live-danmaku-area': '100',
      'live-danmaku-opacity': '0.4',
      'live-danmaku-font-size': '22',
      'live-danmaku-speed': '5',
    })
    const store = useLiveSettingsStore()

    expect(store.danmakuOn).toBe(false)
    expect(store.danmakuArea).toBe(100)
    expect(store.danmakuOpacity).toBe(0.4)
    expect(store.danmakuFontSize).toBe(22)
    expect(store.danmakuSpeed).toBe(5)
  })

  it('clamps out-of-range values and falls back on garbage input', () => {
    installLocalStorage({
      'live-danmaku-area': 'not-a-number',
      'live-danmaku-opacity': '9',
      'live-danmaku-speed': '0',
    })
    const store = useLiveSettingsStore()

    expect(store.danmakuArea).toBe(50)
    expect(store.danmakuOpacity).toBe(1)
    expect(store.danmakuSpeed).toBe(4)

    store.setDanmakuFontSize(99)
    expect(store.danmakuFontSize).toBe(24)
    store.setDanmakuOpacity(0)
    expect(store.danmakuOpacity).toBe(0.2)
    store.setDanmakuArea(10)
    expect(store.danmakuArea).toBe(25)
  })
})
