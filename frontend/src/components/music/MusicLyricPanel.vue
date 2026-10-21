<template>
  <aside class="lyric-panel">
    <header class="lyric-head">
      <strong>歌词</strong>
      <button type="button" class="lyric-close" title="关闭歌词" @click="emit('close')">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M18 6 6 18M6 6l12 12" />
        </svg>
      </button>
    </header>

    <div class="lyric-body">
      <p v-if="loading" class="lyric-state">正在加载歌词…</p>
      <p v-else-if="error" class="lyric-state">{{ error }}</p>
      <p v-else-if="!lines.length" class="lyric-state">暂无歌词</p>
      <template v-else>
        <p
          v-for="(line, index) in lines"
          :key="`${line.time}-${index}`"
          :ref="(el) => setLineRef(el, index)"
          class="lyric-line"
          :class="{ active: index === currentIndex }"
          @click="emit('seek', line.time)"
        >
          <span>{{ line.text }}</span>
          <small v-if="line.translation">{{ line.translation }}</small>
        </p>
      </template>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { watch } from 'vue'
import type { LyricLine } from '../../composables/useMusicLyric'

const props = defineProps<{
  lines: LyricLine[]
  currentIndex: number
  loading: boolean
  error: string
}>()

const emit = defineEmits<{
  close: []
  seek: [seconds: number]
}>()

const lineRefs = new Map<number, HTMLElement>()

function setLineRef(element: unknown, index: number) {
  if (element instanceof HTMLElement) lineRefs.set(index, element)
  else lineRefs.delete(index)
}

watch(
  () => props.currentIndex,
  (index) => {
    if (index < 0) return
    const element = lineRefs.get(index)
    element?.scrollIntoView({ block: 'center', behavior: 'smooth' })
  },
)
</script>

<style scoped>
.lyric-panel {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: min(340px, 86%);
  display: flex;
  flex-direction: column;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
  background: color-mix(in srgb, var(--color-bg-elevated) 96%, transparent);
  backdrop-filter: blur(12px);
  box-shadow: var(--shadow-lg, 0 12px 30px rgba(0, 0, 0, 0.12));
  z-index: 2;
}

.lyric-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-divider);
  color: var(--color-text);
  font-size: 13px;
}

.lyric-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
}

.lyric-close svg {
  width: 15px;
  height: 15px;
}

.lyric-close:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.lyric-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.lyric-state {
  margin: var(--space-4) 0;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-align: center;
}

.lyric-line {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: 13px;
  line-height: 1.5;
  cursor: pointer;
  transition: color var(--duration-fast), transform var(--duration-fast);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.lyric-line small {
  font-size: 11px;
  opacity: 0.8;
}

.lyric-line:hover {
  color: var(--color-text-secondary);
}

.lyric-line.active {
  color: var(--color-primary);
  font-weight: 600;
  transform: translateX(2px);
}
</style>
