import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { pickQualityIndex, useLiveSettingsStore } from './liveSettings'

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
    expect(store.danmakuArea).toBe(25)
    expect(store.danmakuOpacity).toBe(1)
    expect(store.danmakuFontSize).toBe(15)
    expect(store.danmakuSpeed).toBe(8)
    expect(store.qualityLevel).toBe(2)
    expect(store.followRefreshInterval).toBe(60)

    store.setDanmakuOn(false)
    store.setDanmakuArea(75)
    store.setDanmakuOpacity(0.6)
    store.setDanmakuFontSize(20)
    store.setDanmakuSpeed(12)
    store.setQualityLevel(0)
    store.setFollowRefreshInterval(120)

    expect(values.get('live-danmaku-on')).toBe('false')
    expect(values.get('live-danmaku-area')).toBe('75')
    expect(values.get('live-danmaku-opacity')).toBe('0.6')
    expect(values.get('live-danmaku-font-size')).toBe('20')
    expect(values.get('live-danmaku-speed')).toBe('12')
    expect(values.get('live-quality-level')).toBe('0')
    expect(values.get('live-follow-refresh-interval')).toBe('120')
  })

  it('restores saved preferences', () => {
    installLocalStorage({
      'live-danmaku-on': 'false',
      'live-danmaku-area': '100',
      'live-danmaku-opacity': '0.4',
      'live-danmaku-font-size': '22',
      'live-danmaku-speed': '5',
      'live-follow-refresh-interval': '0',
    })
    const store = useLiveSettingsStore()

    expect(store.danmakuOn).toBe(false)
    expect(store.danmakuArea).toBe(100)
    expect(store.danmakuOpacity).toBe(0.4)
    expect(store.danmakuFontSize).toBe(22)
    expect(store.danmakuSpeed).toBe(5)
    expect(store.followRefreshInterval).toBe(0)
  })

  it('clamps out-of-range values and falls back on garbage input', () => {
    installLocalStorage({
      'live-danmaku-area': 'not-a-number',
      'live-danmaku-opacity': '9',
      'live-danmaku-speed': '0',
      'live-quality-level': '9',
      'live-follow-refresh-interval': 'garbage',
    })
    const store = useLiveSettingsStore()

    expect(store.danmakuArea).toBe(25)
    expect(store.danmakuOpacity).toBe(1)
    expect(store.danmakuSpeed).toBe(4)
    expect(store.qualityLevel).toBe(2)
    expect(store.followRefreshInterval).toBe(60)

    store.setDanmakuFontSize(99)
    expect(store.danmakuFontSize).toBe(24)
    store.setDanmakuOpacity(0)
    expect(store.danmakuOpacity).toBe(0.2)
    store.setDanmakuArea(10)
    expect(store.danmakuArea).toBe(25)
    store.setQualityLevel(9)
    expect(store.qualityLevel).toBe(2)
    store.setFollowRefreshInterval(99999)
    expect(store.followRefreshInterval).toBe(3600)
  })

  it('picks the default quality like simple_live', () => {
    // 列表已按真实码率从高到低排序。
    expect(pickQualityIndex(2, 2)).toBe(0)
    expect(pickQualityIndex(2, 0)).toBe(1)
    expect(pickQualityIndex(2, 1)).toBe(1)
    expect(pickQualityIndex(6, 1)).toBe(3)
    expect(pickQualityIndex(0, 2)).toBe(0)
  })
})
