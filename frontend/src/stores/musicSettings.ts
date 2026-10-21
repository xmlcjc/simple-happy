import { defineStore } from 'pinia'
import { ref } from 'vue'

/**
 * 音乐播放偏好：纯前端偏好，只落 localStorage，后端零改动。
 * 与 liveSettings 保持同一套模式（ref + 读写 localStorage）。
 *
 * - playMode: 顺序 / 单曲循环 / 随机。
 * - playbackRate: 播放倍速（0.5 ~ 2）。
 * - preferredQuality: 首选音质，取流失败时按 qualityOrder 逐档降级。
 * - qualityOrder: 降级方向，desc = 往低音质试，asc = 往高音质试。
 * - alternativeMap: 换源配置，key 为音源平台，value 为替代音源平台。
 * - lyricOffsetMs: 歌词偏移（毫秒，正数 = 歌词延后）。
 * - lyricFontSize: 歌词字号档位。
 */

export type MusicPlayMode = 'sequence' | 'repeat-one' | 'shuffle'
export type MusicQuality = 'low' | 'standard' | 'high' | 'super'
export type MusicQualityOrder = 'asc' | 'desc'
export type LyricFontSize = 'small' | 'standard' | 'large' | 'xlarge'

export const PLAY_MODE_OPTIONS: Array<{ value: MusicPlayMode; label: string }> = [
  { value: 'sequence', label: '顺序' },
  { value: 'repeat-one', label: '单曲' },
  { value: 'shuffle', label: '随机' },
]

/** 音质由低到高排列；降级链以此顺序为基准。 */
export const MUSIC_QUALITY_ORDER: MusicQuality[] = ['low', 'standard', 'high', 'super']

export const MUSIC_QUALITY_OPTIONS: Array<{ value: MusicQuality; label: string }> = [
  { value: 'low', label: '低' },
  { value: 'standard', label: '标准' },
  { value: 'high', label: '高' },
  { value: 'super', label: '超高' },
]

export const PLAYBACK_RATES = [0.5, 0.75, 1, 1.25, 1.5, 2] as const

export const LYRIC_FONT_SIZE_OPTIONS: Array<{
  value: LyricFontSize
  label: string
  px: number
}> = [
  { value: 'small', label: '小', px: 12 },
  { value: 'standard', label: '标准', px: 14 },
  { value: 'large', label: '大', px: 16 },
  { value: 'xlarge', label: '超大', px: 18 },
]

export const LYRIC_OFFSET_RANGE = { min: -5000, max: 5000, step: 500 } as const

const PLAY_MODE_KEY = 'music-play-mode'
const PLAYBACK_RATE_KEY = 'music-playback-rate'
const PREFERRED_QUALITY_KEY = 'music-quality'
const QUALITY_ORDER_KEY = 'music-quality-order'
const ALTERNATIVE_KEY = 'music-alternative-plugin'
const LYRIC_OFFSET_KEY = 'music-lyric-offset'
const LYRIC_FONT_SIZE_KEY = 'music-lyric-font-size'

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value))
}

function readNumber(key: string, fallback: number) {
  const raw = localStorage.getItem(key)
  if (raw === null) return fallback
  const value = Number(raw)
  return Number.isFinite(value) ? value : fallback
}

function readEnum<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  const raw = localStorage.getItem(key)
  return allowed.includes(raw as T) ? (raw as T) : fallback
}

function readJson<T>(key: string, fallback: T): T {
  const raw = localStorage.getItem(key)
  if (!raw) return fallback
  try {
    const parsed = JSON.parse(raw)
    return parsed && typeof parsed === 'object' ? (parsed as T) : fallback
  } catch {
    return fallback
  }
}

/** 顺序播放/单曲循环固定取下一首；随机播放在一轮内不重复。返回 -1 表示没有下一首。 */
export function nextTrackIndex(
  mode: MusicPlayMode,
  index: number,
  length: number,
  played: number[] = [],
): number {
  if (length <= 0) return -1
  if (mode !== 'shuffle') {
    return index + 1 < length ? index + 1 : -1
  }
  const candidates: number[] = []
  for (let i = 0; i < length; i += 1) {
    if (i !== index && !played.includes(i)) candidates.push(i)
  }
  if (!candidates.length) {
    // 一轮播完：除当前曲外重新随机，避免单曲队列卡死。
    for (let i = 0; i < length; i += 1) {
      if (i !== index) candidates.push(i)
    }
    if (!candidates.length) return -1
  }
  return candidates[Math.floor(Math.random() * candidates.length)]
}

/**
 * 音质降级链：首选音质打头，再按 order 方向依次尝试，越界后从另一端继续，
 * 保证每个档位只试一次。
 */
export function qualityChain(
  preferred: MusicQuality,
  order: MusicQualityOrder,
): MusicQuality[] {
  const start = MUSIC_QUALITY_ORDER.indexOf(preferred)
  if (start < 0) return [...MUSIC_QUALITY_ORDER]
  const step = order === 'desc' ? -1 : 1
  const chain: MusicQuality[] = []
  for (let i = 0; i < MUSIC_QUALITY_ORDER.length; i += 1) {
    const raw = start + step * i
    const index = ((raw % MUSIC_QUALITY_ORDER.length) + MUSIC_QUALITY_ORDER.length)
      % MUSIC_QUALITY_ORDER.length
    chain.push(MUSIC_QUALITY_ORDER[index])
  }
  return chain
}

/** 倍速按档位循环（0.5 → ... → 2 → 0.5），保留两位小数避免浮点尾数。 */
export function nextPlaybackRate(current: number): number {
  const list = PLAYBACK_RATES
  const index = list.findIndex((rate) => Math.abs(rate - current) < 0.001)
  const next = list[(index + 1) % list.length]
  return index < 0 ? 1 : next
}

export const useMusicSettingsStore = defineStore('musicSettings', () => {
  const playMode = ref<MusicPlayMode>(
    readEnum(PLAY_MODE_KEY, ['sequence', 'repeat-one', 'shuffle'], 'sequence'),
  )
  const playbackRate = ref(clamp(readNumber(PLAYBACK_RATE_KEY, 1), 0.5, 2))
  const preferredQuality = ref<MusicQuality>(
    readEnum(PREFERRED_QUALITY_KEY, MUSIC_QUALITY_ORDER, 'standard'),
  )
  const qualityOrder = ref<MusicQualityOrder>(
    readEnum(QUALITY_ORDER_KEY, ['asc', 'desc'], 'desc'),
  )
  const alternativeMap = ref<Record<string, string>>(readJson(ALTERNATIVE_KEY, {}))
  const lyricOffsetMs = ref(
    clamp(
      Math.round(readNumber(LYRIC_OFFSET_KEY, 0)),
      LYRIC_OFFSET_RANGE.min,
      LYRIC_OFFSET_RANGE.max,
    ),
  )
  const lyricFontSize = ref<LyricFontSize>(
    readEnum(LYRIC_FONT_SIZE_KEY, ['small', 'standard', 'large', 'xlarge'], 'standard'),
  )

  function setPlayMode(mode: MusicPlayMode) {
    playMode.value = mode
    localStorage.setItem(PLAY_MODE_KEY, mode)
  }

  function cyclePlayMode() {
    const index = PLAY_MODE_OPTIONS.findIndex((item) => item.value === playMode.value)
    const next = PLAY_MODE_OPTIONS[(index + 1) % PLAY_MODE_OPTIONS.length].value
    setPlayMode(next)
  }

  function setPlaybackRate(rate: number) {
    if (!Number.isFinite(rate)) return
    playbackRate.value = Math.round(clamp(rate, 0.5, 2) * 100) / 100
    localStorage.setItem(PLAYBACK_RATE_KEY, String(playbackRate.value))
  }

  function setPreferredQuality(quality: MusicQuality) {
    preferredQuality.value = quality
    localStorage.setItem(PREFERRED_QUALITY_KEY, quality)
  }

  function setQualityOrder(order: MusicQualityOrder) {
    qualityOrder.value = order
    localStorage.setItem(QUALITY_ORDER_KEY, order)
  }

  function setAlternative(platform: string, alternative: string) {
    const next = { ...alternativeMap.value }
    if (alternative) next[platform] = alternative
    else delete next[platform]
    alternativeMap.value = next
    localStorage.setItem(ALTERNATIVE_KEY, JSON.stringify(next))
  }

  function setLyricOffsetMs(value: number) {
    if (!Number.isFinite(value)) return
    lyricOffsetMs.value = clamp(
      Math.round(value / LYRIC_OFFSET_RANGE.step) * LYRIC_OFFSET_RANGE.step,
      LYRIC_OFFSET_RANGE.min,
      LYRIC_OFFSET_RANGE.max,
    )
    localStorage.setItem(LYRIC_OFFSET_KEY, String(lyricOffsetMs.value))
  }

  function setLyricFontSize(size: LyricFontSize) {
    lyricFontSize.value = size
    localStorage.setItem(LYRIC_FONT_SIZE_KEY, size)
  }

  return {
    playMode,
    playbackRate,
    preferredQuality,
    qualityOrder,
    alternativeMap,
    lyricOffsetMs,
    lyricFontSize,
    setPlayMode,
    cyclePlayMode,
    setPlaybackRate,
    setPreferredQuality,
    setQualityOrder,
    setAlternative,
    setLyricOffsetMs,
    setLyricFontSize,
  }
})
