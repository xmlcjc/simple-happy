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
  const danmakuArea = ref(clamp(readNumber(DANMAKU_AREA_KEY, 50), 25, 100))
  const danmakuOpacity = ref(
    clamp(readNumber(DANMAKU_OPACITY_KEY, 1), DANMAKU_OPACITY_RANGE.min, DANMAKU_OPACITY_RANGE.max),
  )
  const danmakuFontSize = ref(
    clamp(readNumber(DANMAKU_FONT_SIZE_KEY, 16), DANMAKU_FONT_SIZE_RANGE.min, DANMAKU_FONT_SIZE_RANGE.max),
  )
  const danmakuSpeed = ref(
    clamp(readNumber(DANMAKU_SPEED_KEY, 9), DANMAKU_SPEED_RANGE.min, DANMAKU_SPEED_RANGE.max),
  )

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

  return {
    danmakuOn,
    danmakuArea,
    danmakuOpacity,
    danmakuFontSize,
    danmakuSpeed,
    setDanmakuOn,
    setDanmakuArea,
    setDanmakuOpacity,
    setDanmakuFontSize,
    setDanmakuSpeed,
  }
})
