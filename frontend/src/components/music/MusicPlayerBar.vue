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
        <small>{{ current?.artist || '从搜索结果里选一首歌' }}</small>
      </span>
    </div>

    <div class="player-center">
      <div class="player-controls">
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
import type { MusicItem } from '../../api/music'

defineProps<{
  current: MusicItem | null
  playing: boolean
  loading: boolean
  currentTime: number
  duration: number
  volume: number
  errorText: string
  lyricOpen: boolean
}>()

const emit = defineEmits<{
  toggle: []
  next: []
  prev: []
  seek: [seconds: number]
  volume: [value: number]
  'toggle-lyric': []
}>()

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
