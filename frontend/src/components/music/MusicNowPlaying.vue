<template>
  <Teleport to="body">
    <Transition name="np-fade">
      <div v-if="modelValue" class="np-overlay" @click.self="emit('close')">
        <!-- 背景: 当前封面放大模糊, 主流播放器的 Now Playing 都是这个套路。 -->
        <div v-if="current?.artwork" class="np-bg" :style="{ backgroundImage: `url(${current.artwork})` }"></div>
        <div class="np-scrim"></div>

        <header class="np-head">
          <span class="np-heading">正在播放</span>
          <button type="button" class="np-close" title="收起（Esc）" @click="emit('close')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M6 9l6 6 6-6" />
            </svg>
          </button>
        </header>

        <main class="np-body">
          <section class="np-cover-wrap">
            <div class="np-cover-ring" :class="{ spinning: playing }">
              <img
                v-if="current?.artwork"
                class="np-cover"
                :src="current.artwork"
                :alt="current.title"
                referrerpolicy="no-referrer"
              />
              <div v-else class="np-cover placeholder">♪</div>
            </div>
            <div class="np-meta">
              <h1>{{ current?.title || '未在播放' }}</h1>
              <p>{{ current?.artist || '' }}</p>
            </div>
            <div class="np-actions">
              <button
                type="button"
                class="np-action"
                :class="{ active: favorited }"
                :disabled="!current"
                @click="emit('toggle-favorite')"
              >{{ favorited ? '♥ 已收藏' : '♡ 收藏' }}</button>
              <button type="button" class="np-action" :disabled="!current" @click="emit('add-to-playlist')">
                ＋ 加入歌单
              </button>
            </div>
          </section>

          <section class="np-lyrics">
            <p v-if="lyricLoading" class="np-lyric-state">正在加载歌词…</p>
            <p v-else-if="lyricError" class="np-lyric-state">{{ lyricError }}</p>
            <p v-else-if="!lines.length" class="np-lyric-state">暂无歌词</p>
            <template v-else>
              <p
                v-for="(line, index) in lines"
                :key="`${line.time}-${index}`"
                :ref="(el) => setLineRef(el, index)"
                class="np-lyric-line"
                :class="{ active: index === currentIndex }"
                @click="emit('seek', line.time)"
              >
                <span>{{ line.text }}</span>
                <small v-if="line.translation">{{ line.translation }}</small>
              </p>
            </template>
          </section>
        </main>

        <footer class="np-foot">
          <div class="np-progress">
            <span class="np-time">{{ formatTime(currentTime) }}</span>
            <input
              class="np-slider"
              type="range"
              min="0"
              :max="duration || 1"
              step="0.1"
              :value="Math.min(currentTime, duration || 1)"
              :disabled="!current || !duration"
              aria-label="播放进度"
              @input="emit('seek', Number(($event.target as HTMLInputElement).value))"
            />
            <span class="np-time">{{ formatTime(duration) }}</span>
          </div>
          <div class="np-controls">
            <button type="button" class="np-ctrl" title="上一首" :disabled="!current" @click="emit('prev')">
              <svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 5h2v14H6zM20 5v14L9 12z" /></svg>
            </button>
            <button
              type="button"
              class="np-ctrl primary"
              :title="playing ? '暂停' : '播放'"
              :disabled="!current || loading"
              @click="emit('toggle')"
            >
              <svg v-if="loading" class="np-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 12a9 9 0 1 0-3 6.7" />
              </svg>
              <svg v-else-if="playing" viewBox="0 0 24 24" fill="currentColor"><path d="M7 5h4v14H7zM13 5h4v14h-4z" /></svg>
              <svg v-else viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
            </button>
            <button type="button" class="np-ctrl" title="下一首" :disabled="!current" @click="emit('next')">
              <svg viewBox="0 0 24 24" fill="currentColor"><path d="M16 5h2v14h-2zM4 5v14l11-7z" /></svg>
            </button>
          </div>
        </footer>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { onBeforeUnmount, watch } from 'vue'
import type { MusicItem } from '../../api/music'
import type { LyricLine } from '../../composables/useMusicLyric'

const props = defineProps<{
  modelValue: boolean
  current: MusicItem | null
  playing: boolean
  loading: boolean
  currentTime: number
  duration: number
  lines: LyricLine[]
  currentIndex: number
  lyricLoading: boolean
  lyricError: string
  favorited: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  close: []
  toggle: []
  next: []
  prev: []
  seek: [seconds: number]
  'toggle-favorite': []
  'add-to-playlist': []
}>()

const lineRefs = new Map<number, HTMLElement>()

function setLineRef(element: unknown, index: number) {
  if (element instanceof HTMLElement) lineRefs.set(index, element)
  else lineRefs.delete(index)
}

watch(
  () => props.currentIndex,
  (index) => {
    if (!props.modelValue || index < 0) return
    lineRefs.get(index)?.scrollIntoView({ block: 'center', behavior: 'smooth' })
  },
)

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && props.modelValue) {
    event.preventDefault()
    emit('close')
  }
}

watch(
  () => props.modelValue,
  (open) => {
    if (typeof window === 'undefined') return
    if (open) window.addEventListener('keydown', onKeydown)
    else window.removeEventListener('keydown', onKeydown)
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  if (typeof window !== 'undefined') window.removeEventListener('keydown', onKeydown)
})

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds <= 0) return '0:00'
  const total = Math.round(seconds)
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`
}
</script>

<style scoped>
.np-overlay {
  position: fixed;
  top: var(--titlebar-height, 30px);
  right: 0;
  bottom: 0;
  left: 0;
  z-index: 100;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: #fff;
  background: #14100d;
}

.np-bg {
  position: absolute;
  inset: -40px;
  background-position: center;
  background-size: cover;
  filter: blur(60px) saturate(140%);
  transform: scale(1.15);
  opacity: 0.55;
  pointer-events: none;
}

.np-scrim {
  position: absolute;
  inset: 0;
  background: linear-gradient(180deg, rgba(10, 8, 6, 0.55), rgba(10, 8, 6, 0.82));
  pointer-events: none;
}

.np-head {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 22px;
}

.np-heading {
  font-size: 12px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: rgba(255, 255, 255, 0.55);
}

.np-close {
  width: 34px;
  height: 34px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  color: rgba(255, 255, 255, 0.8);
  background: rgba(255, 255, 255, 0.1);
}

.np-close:hover {
  background: rgba(255, 255, 255, 0.18);
  color: #fff;
}

.np-close svg {
  width: 18px;
  height: 18px;
}

.np-body {
  position: relative;
  z-index: 1;
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(280px, 420px) minmax(0, 1fr);
  align-items: center;
  gap: clamp(24px, 5vw, 72px);
  padding: 0 clamp(24px, 6vw, 80px);
}

.np-cover-wrap {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 18px;
  min-width: 0;
}

.np-cover-ring {
  width: min(100%, 340px);
  aspect-ratio: 1;
  border-radius: 50%;
  overflow: hidden;
  box-shadow: 0 26px 70px rgba(0, 0, 0, 0.5);
  border: 1px solid rgba(255, 255, 255, 0.16);
  animation: np-spin 26s linear infinite;
  animation-play-state: paused;
}

.np-cover-ring.spinning {
  animation-play-state: running;
}

.np-cover {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  border-radius: 50%;
}

.np-cover.placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 72px;
  color: rgba(255, 255, 255, 0.5);
  background: rgba(255, 255, 255, 0.08);
}

.np-meta {
  text-align: center;
  max-width: 100%;
}

.np-meta h1 {
  margin: 0;
  font-size: clamp(18px, 2vw, 26px);
  line-height: 1.3;
  font-weight: 700;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.np-meta p {
  margin: 6px 0 0;
  color: rgba(255, 255, 255, 0.6);
  font-size: 14px;
}

.np-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.np-action {
  padding: 7px 16px;
  border-radius: 999px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  color: rgba(255, 255, 255, 0.82);
  font-size: 12px;
  transition: all var(--duration-fast) var(--ease-out);
}

.np-action:hover:not(:disabled) {
  border-color: rgba(255, 255, 255, 0.45);
  color: #fff;
}

.np-action.active {
  border-color: var(--color-primary);
  background: rgba(201, 127, 58, 0.22);
  color: #ffd6ac;
}

.np-action:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.np-lyrics {
  min-height: 0;
  max-height: 100%;
  overflow-y: auto;
  padding: 12px 6px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  mask-image: linear-gradient(180deg, transparent, #000 12%, #000 88%, transparent);
  scrollbar-width: none;
}

.np-lyrics::-webkit-scrollbar {
  display: none;
}

.np-lyric-state {
  margin: auto;
  color: rgba(255, 255, 255, 0.5);
  font-size: 13px;
  text-align: center;
}

.np-lyric-line {
  margin: 0;
  color: rgba(255, 255, 255, 0.42);
  font-size: 17px;
  line-height: 1.5;
  text-align: center;
  cursor: pointer;
  transition: color var(--duration-fast), transform var(--duration-fast);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.np-lyric-line small {
  font-size: 13px;
  opacity: 0.7;
}

.np-lyric-line:hover {
  color: rgba(255, 255, 255, 0.7);
}

.np-lyric-line.active {
  color: #fff;
  font-size: 20px;
  font-weight: 700;
  transform: scale(1.02);
  text-shadow: 0 0 26px rgba(255, 190, 130, 0.45);
}

.np-foot {
  position: relative;
  z-index: 1;
  padding: 10px clamp(24px, 6vw, 80px) 22px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.np-progress {
  display: flex;
  align-items: center;
  gap: 12px;
}

.np-time {
  flex-shrink: 0;
  color: rgba(255, 255, 255, 0.55);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.np-slider {
  flex: 1;
  appearance: none;
  -webkit-appearance: none;
  height: 18px;
  background: transparent;
  cursor: pointer;
}

.np-slider::-webkit-slider-runnable-track {
  height: 4px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.22);
}

.np-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 12px;
  height: 12px;
  margin-top: -4px;
  border-radius: 50%;
  background: #fff;
}

.np-slider::-moz-range-track {
  height: 4px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.22);
}

.np-slider::-moz-range-thumb {
  width: 12px;
  height: 12px;
  border: none;
  border-radius: 50%;
  background: #fff;
}

.np-controls {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 16px;
}

.np-ctrl {
  width: 40px;
  height: 40px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  color: rgba(255, 255, 255, 0.85);
}

.np-ctrl svg {
  width: 20px;
  height: 20px;
}

.np-ctrl:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.12);
  color: #fff;
}

.np-ctrl.primary {
  width: 56px;
  height: 56px;
  background: #fff;
  color: #1a1410;
}

.np-ctrl.primary svg {
  width: 24px;
  height: 24px;
}

.np-ctrl:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.np-spin {
  animation: np-spin-icon 0.9s linear infinite;
}

@keyframes np-spin-icon {
  to { transform: rotate(360deg); }
}

@keyframes np-spin {
  to { transform: rotate(360deg); }
}

.np-fade-enter-active,
.np-fade-leave-active {
  transition: opacity var(--duration-normal) var(--ease-out);
}

.np-fade-enter-from,
.np-fade-leave-to {
  opacity: 0;
}

@media (max-width: 760px) {
  .np-body {
    grid-template-columns: 1fr;
    align-items: start;
    gap: 18px;
    padding-top: 6px;
    overflow-y: auto;
  }

  .np-cover-ring {
    width: min(46vw, 200px);
  }

  .np-lyrics {
    max-height: 46vh;
  }
}
</style>
