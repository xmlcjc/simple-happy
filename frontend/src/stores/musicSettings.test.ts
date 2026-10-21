import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import {
  LYRIC_OFFSET_RANGE,
  MUSIC_QUALITY_ORDER,
  nextPlaybackRate,
  nextTrackIndex,
  qualityChain,
  useMusicSettingsStore,
} from './musicSettings'

function installLocalStorage(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial))
  vi.stubGlobal('localStorage', {
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => values.set(key, value)),
    removeItem: vi.fn((key: string) => values.delete(key)),
  })
  return values
}

describe('music settings store', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    setActivePinia(createPinia())
  })

  it('uses sensible defaults and persists every change', () => {
    const values = installLocalStorage()
    const store = useMusicSettingsStore()

    expect(store.playMode).toBe('sequence')
    expect(store.playbackRate).toBe(1)
    expect(store.preferredQuality).toBe('standard')
    expect(store.qualityOrder).toBe('desc')
    expect(store.alternativeMap).toEqual({})
    expect(store.lyricOffsetMs).toBe(0)
    expect(store.lyricFontSize).toBe('standard')

    store.setPlayMode('shuffle')
    store.setPlaybackRate(1.25)
    store.setPreferredQuality('high')
    store.setQualityOrder('asc')
    store.setAlternative('a', 'b')
    store.setLyricOffsetMs(1200)
    store.setLyricFontSize('large')

    expect(values.get('music-play-mode')).toBe('shuffle')
    expect(values.get('music-playback-rate')).toBe('1.25')
    expect(values.get('music-quality')).toBe('high')
    expect(values.get('music-quality-order')).toBe('asc')
    expect(values.get('music-alternative-plugin')).toBe('{"a":"b"}')
    expect(values.get('music-lyric-offset')).toBe('1000')
    expect(values.get('music-lyric-font-size')).toBe('large')
  })

  it('restores saved preferences and drops unknown enum values', () => {
    installLocalStorage({
      'music-play-mode': 'nonsense',
      'music-playback-rate': '1.5',
      'music-quality': 'high',
      'music-quality-order': 'asc',
      'music-alternative-plugin': '{"x":"y"}',
      'music-lyric-offset': '-2500',
      'music-lyric-font-size': 'xlarge',
    })
    const store = useMusicSettingsStore()

    expect(store.playMode).toBe('sequence')
    expect(store.playbackRate).toBe(1.5)
    expect(store.preferredQuality).toBe('high')
    expect(store.qualityOrder).toBe('asc')
    expect(store.alternativeMap).toEqual({ x: 'y' })
    expect(store.lyricOffsetMs).toBe(-2500)
    expect(store.lyricFontSize).toBe('xlarge')
  })

  it('clamps out-of-range values and falls back on garbage input', () => {
    installLocalStorage({
      'music-playback-rate': '9',
      'music-lyric-offset': 'garbage',
      'music-alternative-plugin': 'not-json',
    })
    const store = useMusicSettingsStore()

    expect(store.playbackRate).toBe(2)
    expect(store.lyricOffsetMs).toBe(0)
    expect(store.alternativeMap).toEqual({})

    store.setPlaybackRate(0.01)
    expect(store.playbackRate).toBe(0.5)
    store.setLyricOffsetMs(99999)
    expect(store.lyricOffsetMs).toBe(LYRIC_OFFSET_RANGE.max)
    store.setAlternative('a', '')
    expect(store.alternativeMap).toEqual({})
  })

  it('cycles the play mode in order', () => {
    installLocalStorage()
    const store = useMusicSettingsStore()

    store.cyclePlayMode()
    expect(store.playMode).toBe('repeat-one')
    store.cyclePlayMode()
    expect(store.playMode).toBe('shuffle')
    store.cyclePlayMode()
    expect(store.playMode).toBe('sequence')
  })

  it('advances sequentially and stops at the end', () => {
    expect(nextTrackIndex('sequence', 0, 3)).toBe(1)
    expect(nextTrackIndex('repeat-one', 1, 3)).toBe(2)
    expect(nextTrackIndex('sequence', 2, 3)).toBe(-1)
    expect(nextTrackIndex('sequence', 0, 0)).toBe(-1)
  })

  it('never repeats an already played track during one shuffle round', () => {
    const seen = new Set<number>()
    for (let i = 0; i < 40; i += 1) {
      const target = nextTrackIndex('shuffle', 0, 4, [0, 1, 2])
      expect(target).toBe(3)
      seen.add(target)
    }
    expect([...seen]).toEqual([3])
  })

  it('reshuffles when every track has been played', () => {
    const target = nextTrackIndex('shuffle', 0, 3, [0, 1, 2])
    expect([1, 2]).toContain(target)
    expect(nextTrackIndex('shuffle', 0, 1, [0])).toBe(-1)
  })

  it('builds a quality fallback chain that covers every tier once', () => {
    expect(qualityChain('high', 'desc')).toEqual(['high', 'standard', 'low', 'super'])
    expect(qualityChain('high', 'asc')).toEqual(['high', 'super', 'low', 'standard'])
    expect(qualityChain('low', 'desc')).toEqual(['low', 'super', 'high', 'standard'])
    expect(new Set(qualityChain('standard', 'desc')).size).toBe(MUSIC_QUALITY_ORDER.length)
  })

  it('cycles playback rate and resets unknown values to 1x', () => {
    expect(nextPlaybackRate(1)).toBe(1.25)
    expect(nextPlaybackRate(2)).toBe(0.5)
    expect(nextPlaybackRate(0.5)).toBe(0.75)
    expect(nextPlaybackRate(1.1)).toBe(1)
  })
})
