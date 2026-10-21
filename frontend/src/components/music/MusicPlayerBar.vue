<template>
  <div class="player-bar" :class="{ empty: !current }">
    <div class="player-now">
      <img
        v-if="current?.artwork"
        class="now-cover"
        :src="current.artwork"
        :alt="current.title"
        referrerpolicy="no-referrer"
      />
      <span v-else class="now-cover placeholder">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
          <path d="M9 18V6l10-2v12" />
          <circle cx="6" cy="18" r="3" />
          <circle cx="16" cy="16" r="3" />
        </svg>
      </span>
      <span class="now-text">
        <strong>{{ current?.title || '未在播放' }}</strong>
        <small>{{ current?.artist || '从搜索结果里选一首歌' }}<template v-if="swappedFrom"> · 已从 {{ swappedFrom }} 换源</template></small>
      </span>
    </div>

    <div class="player-center">
      <div class="player-controls">
        <button
          type="button"
          class="mini-btn mode-btn"
          :title="`播放模式：${modeLabel}（点击切换）`"
          @click="emit('cycle-mode')"
        >{{ modeLabel }}</button>
        <button type="button" class="ctrl-btn" title="上一首" :disabled="!current" @click="emit('prev')">
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 5h2v14H6zM20 5v14L9 12z" /></svg>
        </button>
        <button
          type="button"
          class="ctrl-btn primary"
          :title="playing ? '暂停' : '播放'"
          :disabled="!current || loading"
          @click="emit('toggle')"
        >
          <svg v-if="loading" class="spinning" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 12a9 9 0 1 0-3 6.7" />
          </svg>
          <svg v-else-if="playing" viewBox="0 0 24 24" fill="currentColor"><path d="M7 5h4v14H7zM13 5h4v14h-4z" /></svg>
          <svg v-else viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
        </button>
        <button type="button" class="ctrl-btn" title="下一首" :disabled="!current" @click="emit('next')">
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M16 5h2v14h-2zM4 5v14l11-7z" /></svg>
        </button>
      </div>
      <div class="player-progress">
        <span class="time">{{ formatTime(currentTime) }}</span>
        <input
          class="progress-slider"
          type="range"
          min="0"
          :max="duration || 1"
          step="0.1"
          :value="Math.min(currentTime, duration || 1)"
          :disabled="!current || !duration"
          aria-label="播放进度"
          @input="emit('seek', Number(($event.target as HTMLInputElement).value))"
        />
        <span class="time">{{ formatTime(duration) }}</span>
      </div>
    </div>

    <div class="player-extra">
      <div class="quality-wrap">
        <button
          type="button"
          class="mini-btn"
          :title="`音质：${qualityLabel}（点击选择）`"
          @click="qualityOpen = !qualityOpen"
        >{{ qualityLabel }}</button>
        <div v-if="qualityOpen" class="quality-menu" role="menu" aria-label="音质选择">
          <button
            v-for="option in MUSIC_QUALITY_OPTIONS"
            :key="option.value"
            type="button"
            class="quality-row"
            role="menuitemradio"
            :aria-checked="option.value === preferredQuality"
            :class="{ active: option.value === preferredQuality }"
            @click="chooseQuality(option.value)"
          >{{ option.label }}</button>
          <button
            type="button"
            class="quality-row order"
            role="menuitem"
            @click="emit('set-quality-order', qualityOrder === 'desc' ? 'asc' : 'desc')"
          >失败时优先{{ qualityOrder === 'desc' ? '低' : '高' }}音质</button>
        </div>
      </div>
      <div class="quality-wrap">
        <button
          type="button"
          class="mini-btn"
          :class="{ active: sleepMode !== 'off' }"
          :title="sleepMode === 'off' ? '定时关闭' : `定时关闭：${sleepLabel}`"
          @click="sleepOpen = !sleepOpen"
        >{{ sleepLabel }}</button>
        <div v-if="sleepOpen" class="quality-menu" role="menu" aria-label="定时关闭">
          <button
            v-for="minutes in [15, 30, 60]"
            :key="minutes"
            type="button"
            class="quality-row"
            role="menuitem"
            @click="chooseSleep(minutes)"
          >{{ minutes }} 分钟后</button>
          <button
            type="button"
            class="quality-row"
            role="menuitem"
            @click="emit('sleep-end'); sleepOpen = false"
          >播完当前曲</button>
          <button
            v-if="sleepMode !== 'off'"
            type="button"
            class="quality-row order"
            role="menuitem"
            @click="emit('cancel-sleep'); sleepOpen = false"
          >取消定时</button>
        </div>
      </div>
      <button
        type="button"
        class="mini-btn"
        :class="{ active: queueOpen }"
        :title="`播放队列（${queueCount} 首）`"
        @click="emit('toggle-queue')"
      >队列 {{ queueCount }}</button>
      <button
        type="button"
        class="mini-btn"
        title="播放速度（点击切换）"
        @click="emit('cycle-rate')"
      >{{ rateLabel }}</button>
      <button
        type="button"
        class="lyric-btn"
        :class="{ active: lyricOpen }"
        title="歌词"
        @click="emit('toggle-lyric')"
      >词</button>
      <svg class="volume-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M11 5 6 9H3v6h3l5 4z" />
        <path d="M15.5 8.5a5 5 0 0 1 0 7" />
      </svg>
      <input
        class="volume-slider"
        type="range"
        min="0"
        max="1"
        step="0.01"
        :value="volume"
        aria-label="音量"
        @input="emit('volume', Number(($event.target as HTMLInputElement).value))"
      />
    </div>

    <p v-if="errorText" class="player-error">{{ errorText }}</p>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import {
  MUSIC_QUALITY_OPTIONS,
  PLAY_MODE_OPTIONS,
  type MusicPlayMode,
  type MusicQuality,
  type MusicQualityOrder,
} from '../../stores/musicSettings'
import type { MusicItem } from '../../api/music'

const props = defineProps<{
  current: MusicItem | null
  playing: boolean
  loading: boolean
  currentTime: number
  duration: number
  volume: number
  errorText: string
  lyricOpen: boolean
  playMode: MusicPlayMode
  playbackRate: number
  queueCount: number
  queueOpen: boolean
  qualityLabel: string
  preferredQuality: MusicQuality
  qualityOrder: MusicQualityOrder
  swappedFrom: string
  sleepMode: 'off' | 'countdown' | 'end-of-track'
  sleepRemaining: number
}>()

const emit = defineEmits<{
  toggle: []
  next: []
  prev: []
  seek: [seconds: number]
  volume: [value: number]
  'toggle-lyric': []
  'cycle-mode': []
  'cycle-rate': []
  'toggle-queue': []
  'set-quality': [quality: MusicQuality]
  'set-quality-order': [order: MusicQualityOrder]
  'set-sleep': [minutes: number]
  'sleep-end': []
  'cancel-sleep': []
}>()

const qualityOpen = ref(false)
const sleepOpen = ref(false)

function chooseQuality(quality: MusicQuality) {
  qualityOpen.value = false
  emit('set-quality', quality)
}

function chooseSleep(minutes: number) {
  sleepOpen.value = false
  emit('set-sleep', minutes)
}

const sleepLabel = computed(() => {
  if (props.sleepMode === 'end-of-track') return '播完'
  if (props.sleepMode === 'countdown') {
    const total = Math.max(0, props.sleepRemaining)
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`
  }
  return '定时'
})

const modeLabel = computed(
  () => PLAY_MODE_OPTIONS.find((item) => item.value === props.playMode)?.label ?? '顺序',
)
const rateLabel = computed(() => `${props.playbackRate}x`)

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds <= 0) return '0:00'
  const total = Math.round(seconds)
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`
}
</script>

<style scoped>
.player-bar {
  position: relative;
  flex-shrink: 0;
  display: grid;
  grid-template-columns: minmax(0, 1.1fr) minmax(0, 2fr) auto;
  align-items: center;
  gap: var(--space-4);
  margin-top: var(--space-3);
  padding: 10px var(--space-4);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
}

.player-now {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  min-width: 0;
}

.now-cover {
  width: 42px;
  height: 42px;
  border-radius: var(--radius-sm);
  object-fit: cover;
  flex-shrink: 0;
  background: var(--color-bg-sunken);
}

.now-cover.placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
}

.now-cover.placeholder svg {
  width: 20px;
  height: 20px;
}

.now-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.now-text strong,
.now-text small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.now-text strong {
  color: var(--color-text);
  font-size: 13px;
  font-weight: 600;
}

.now-text small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.player-center {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.player-controls {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
}

.ctrl-btn {
  width: 30px;
  height: 30px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  color: var(--color-text-secondary);
}

.ctrl-btn svg {
  width: 16px;
  height: 16px;
}

.ctrl-btn:hover:not(:disabled) {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.ctrl-btn.primary {
  width: 36px;
  height: 36px;
  background: var(--color-primary-soft-bg);
  color: var(--color-primary);
}

.ctrl-btn.primary svg {
  width: 18px;
  height: 18px;
}

.ctrl-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.spinning {
  animation: player-spin 0.9s linear infinite;
}

@keyframes player-spin {
  to { transform: rotate(360deg); }
}

.player-progress {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.time {
  flex-shrink: 0;
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.progress-slider,
.volume-slider {
  appearance: none;
  -webkit-appearance: none;
  height: 16px;
  background: transparent;
  cursor: pointer;
}

.progress-slider {
  flex: 1;
  min-width: 0;
}

.volume-slider {
  width: 74px;
}

.progress-slider::-webkit-slider-runnable-track,
.volume-slider::-webkit-slider-runnable-track {
  height: 4px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--color-text-primary) 18%, transparent);
}

.progress-slider::-webkit-slider-thumb,
.volume-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 12px;
  height: 12px;
  margin-top: -4px;
  border-radius: 50%;
  background: var(--color-primary);
}

.progress-slider::-moz-range-track,
.volume-slider::-moz-range-track {
  height: 4px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--color-text-primary) 18%, transparent);
}

.progress-slider::-moz-range-thumb,
.volume-slider::-moz-range-thumb {
  width: 12px;
  height: 12px;
  border: none;
  border-radius: 50%;
  background: var(--color-primary);
}

.player-extra {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.volume-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  color: var(--color-text-tertiary);
}

.lyric-btn {
  padding: 4px 10px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.mini-btn {
  flex-shrink: 0;
  padding: 3px 8px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.mini-btn:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.mini-btn.active {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
}

.mode-btn {
  min-width: 38px;
}

.quality-wrap {
  position: relative;
  flex-shrink: 0;
}

.quality-menu {
  position: absolute;
  right: 0;
  bottom: calc(100% + 6px);
  min-width: 118px;
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg-elevated);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.18);
  z-index: 5;
}

.quality-row {
  padding: 6px 10px;
  border-radius: var(--radius-sm);
  color: var(--color-text-secondary);
  font-size: 12px;
  text-align: left;
  white-space: nowrap;
}

.quality-row:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.quality-row.active {
  color: var(--color-primary);
  font-weight: 600;
}

.quality-row.order {
  margin-top: 2px;
  border-top: 1px solid var(--color-divider);
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.lyric-btn.active {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
}

.player-error {
  position: absolute;
  left: var(--space-4);
  right: var(--space-4);
  bottom: -18px;
  margin: 0;
  color: var(--color-danger, #c0392b);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (max-width: 900px) {
  .player-bar {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.6fr);
  }

  .player-extra {
    display: none;
  }
}
</style>
