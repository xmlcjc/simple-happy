import { defineStore } from 'pinia'
import { ref } from 'vue'

/**
 * 直播偏好设置：纯前端偏好，只落 localStorage，后端零改动。
 * 模式与 stores/app.ts 的 ref + localStorage 读写一致。
 *
 * - danmakuOn: 进入直播间时弹幕是否默认开启（播放页仍可临时切换，切换会写回）。
 * - danmakuArea: 飘屏弹幕显示区域（画面高度百分比，25/50/75/100）。
 * - danmakuOpacity: 飘屏弹幕不透明度（0.2 ~ 1）。
 * - danmakuFontSize: 飘屏弹幕字号（px，12 ~ 24）。
 * - danmakuSpeed: 飘屏弹幕滚过画面的持续秒数（4 ~ 16，越小越快）。
 */
const DANMAKU_ON_KEY = 'live-danmaku-on'
const DANMAKU_AREA_KEY = 'live-danmaku-area'
const DANMAKU_OPACITY_KEY = 'live-danmaku-opacity'
const DANMAKU_FONT_SIZE_KEY = 'live-danmaku-font-size'
const DANMAKU_SPEED_KEY = 'live-danmaku-speed'
const QUALITY_LEVEL_KEY = 'live-quality-level'
const FOLLOW_REFRESH_INTERVAL_KEY = 'live-follow-refresh-interval'
/** B 站登录 Cookie；为空表示未登录，直播只能拿到「超清」。 */
const BILIBILI_COOKIE_KEY = 'live-bilibili-cookie'

/** 关注列表自动刷新间隔（秒）；0 = 关闭。 */
export const FOLLOW_REFRESH_INTERVAL_OPTIONS = [
  { value: 0, label: '关闭' },
  { value: 30, label: '30秒' },
  { value: 60, label: '1分钟' },
  { value: 120, label: '2分钟' },
  { value: 300, label: '5分钟' },
] as const

/** 默认清晰度: 0=最低(列表最后一个), 1=中等, 2=最高(列表第一个)。 */
export const QUALITY_LEVEL_OPTIONS = [
  { value: 0, label: '最低' },
  { value: 1, label: '中等' },
  { value: 2, label: '最高' },
] as const

/** 按 Simple Live 的规则, 根据清晰度偏好选默认下标。
 *  列表由后端按实际码率从高到低排好; 最高=第一个, 最低=最后一个, 中等=中间。 */
export function pickQualityIndex(count: number, level: number): number {
  if (count <= 0) return 0
  if (level === 2) return 0
  if (level === 0) return count - 1
  return Math.floor(count / 2)
}

export const DANMAKU_AREA_OPTIONS = [25, 50, 75, 100] as const
export const DANMAKU_SPEED_RANGE = { min: 4, max: 16 } as const
export const DANMAKU_FONT_SIZE_RANGE = { min: 12, max: 24 } as const
export const DANMAKU_OPACITY_RANGE = { min: 0.2, max: 1 } as const

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value))
}

function readNumber(key: string, fallback: number) {
  const raw = localStorage.getItem(key)
  if (raw === null) return fallback
  const value = Number(raw)
  return Number.isFinite(value) ? value : fallback
}

export const useLiveSettingsStore = defineStore('liveSettings', () => {
  const danmakuOn = ref(localStorage.getItem(DANMAKU_ON_KEY) !== 'false')
  // 参考主流直播全屏的弹幕: 默认只占画面上部约 1/4, 字号略小、速度稍快。
  const danmakuArea = ref(clamp(readNumber(DANMAKU_AREA_KEY, 25), 25, 100))
  const danmakuOpacity = ref(
    clamp(readNumber(DANMAKU_OPACITY_KEY, 1), DANMAKU_OPACITY_RANGE.min, DANMAKU_OPACITY_RANGE.max),
  )
  const danmakuFontSize = ref(
    clamp(readNumber(DANMAKU_FONT_SIZE_KEY, 15), DANMAKU_FONT_SIZE_RANGE.min, DANMAKU_FONT_SIZE_RANGE.max),
  )
  const danmakuSpeed = ref(
    clamp(readNumber(DANMAKU_SPEED_KEY, 8), DANMAKU_SPEED_RANGE.min, DANMAKU_SPEED_RANGE.max),
  )
  /** 进入直播间默认选哪个清晰度; 默认最高, 避免一进房间就播标清。 */
  const qualityLevel = ref(clamp(readNumber(QUALITY_LEVEL_KEY, 2), 0, 2))
  /** 关注列表自动刷新间隔(秒), 0 = 不自动刷新。 */
  const followRefreshInterval = ref(clamp(readNumber(FOLLOW_REFRESH_INTERVAL_KEY, 60), 0, 3600))
  /** B 站登录 Cookie。登录后取流接口才会下发原画/蓝光等更高档位。 */
  const bilibiliCookie = ref(localStorage.getItem(BILIBILI_COOKIE_KEY) || '')

  function setDanmakuOn(value: boolean) {
    danmakuOn.value = value
    localStorage.setItem(DANMAKU_ON_KEY, String(value))
  }

  function setDanmakuArea(value: number) {
    danmakuArea.value = clamp(Math.round(value), 25, 100)
    localStorage.setItem(DANMAKU_AREA_KEY, String(danmakuArea.value))
  }

  function setDanmakuOpacity(value: number) {
    danmakuOpacity.value = clamp(value, DANMAKU_OPACITY_RANGE.min, DANMAKU_OPACITY_RANGE.max)
    localStorage.setItem(DANMAKU_OPACITY_KEY, String(danmakuOpacity.value))
  }

  function setDanmakuFontSize(value: number) {
    danmakuFontSize.value = clamp(
      Math.round(value),
      DANMAKU_FONT_SIZE_RANGE.min,
      DANMAKU_FONT_SIZE_RANGE.max,
    )
    localStorage.setItem(DANMAKU_FONT_SIZE_KEY, String(danmakuFontSize.value))
  }

  function setDanmakuSpeed(value: number) {
    danmakuSpeed.value = clamp(
      Math.round(value),
      DANMAKU_SPEED_RANGE.min,
      DANMAKU_SPEED_RANGE.max,
    )
    localStorage.setItem(DANMAKU_SPEED_KEY, String(danmakuSpeed.value))
  }

  function setQualityLevel(value: number) {
    qualityLevel.value = clamp(Math.round(value), 0, 2)
    localStorage.setItem(QUALITY_LEVEL_KEY, String(qualityLevel.value))
  }

  function setFollowRefreshInterval(value: number) {
    followRefreshInterval.value = clamp(Math.round(value), 0, 3600)
    localStorage.setItem(FOLLOW_REFRESH_INTERVAL_KEY, String(followRefreshInterval.value))
  }

  function setBilibiliCookie(value: string) {
    bilibiliCookie.value = value.trim()
    localStorage.setItem(BILIBILI_COOKIE_KEY, bilibiliCookie.value)
  }

  return {
    danmakuOn,
    danmakuArea,
    danmakuOpacity,
    danmakuFontSize,
    danmakuSpeed,
    qualityLevel,
    followRefreshInterval,
    bilibiliCookie,
    setDanmakuOn,
    setDanmakuArea,
    setDanmakuOpacity,
    setDanmakuFontSize,
    setDanmakuSpeed,
    setQualityLevel,
    setFollowRefreshInterval,
    setBilibiliCookie,
  }
})
