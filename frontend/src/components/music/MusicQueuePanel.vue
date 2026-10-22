<template>
  <aside class="queue-panel" aria-label="播放队列">
    <header class="queue-head">
      <strong>播放队列</strong>
      <small>{{ items.length }} 首</small>
      <button type="button" class="queue-text-btn" :disabled="!items.length" @click="emit('clear')">
        清空
      </button>
      <button type="button" class="queue-close" title="关闭队列" @click="emit('close')">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M18 6 6 18M6 6l12 12" />
        </svg>
      </button>
    </header>

    <div class="queue-body">
      <p v-if="!items.length" class="queue-empty">队列是空的，从搜索结果里选一首歌开始播放。</p>
      <div
        v-for="(item, index) in items"
        :key="`${item.platform}:${item.id}:${index}`"
        class="queue-row"
        :class="{ active: index === currentIndex }"
      >
        <button type="button" class="queue-main" @click="emit('jump', index)">
          <span class="queue-index">{{ index + 1 }}</span>
          <span class="queue-text">
            <strong>{{ item.title || '未知标题' }}</strong>
            <small>{{ item.artist || '未知歌手' }}</small>
          </span>
          <span v-if="item.duration" class="queue-duration">{{ formatDuration(item.duration) }}</span>
        </button>
        <button type="button" class="queue-remove" title="从队列移除" @click="emit('remove', index)">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M6 6l12 12M18 6 6 18" />
          </svg>
        </button>
      </div>
    </div>
  </aside>
</template>

<script setup lang="ts">
import type { MusicItem } from '../../api/music'

defineProps<{
  items: MusicItem[]
  currentIndex: number
}>()

const emit = defineEmits<{
  close: []
  jump: [index: number]
  remove: [index: number]
  clear: []
}>()

function formatDuration(seconds: number) {
  const total = Math.max(0, Math.round(seconds))
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`
}
</script>

<style scoped>
.queue-panel {
  position: fixed;
  right: 12px;
  bottom: 80px;
  z-index: 98;
  width: min(360px, calc(100vw - 24px));
  max-height: min(520px, calc(100dvh - 120px));
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
  background: color-mix(in srgb, var(--color-bg-elevated) 96%, transparent);
  backdrop-filter: blur(12px);
  box-shadow: var(--shadow-lg, 0 12px 30px rgba(0, 0, 0, 0.12));
}

.queue-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-divider);
}

.queue-head strong {
  color: var(--color-text);
  font-size: 13px;
}

.queue-head small {
  flex: 1;
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.queue-text-btn {
  padding: 3px 10px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.queue-text-btn:hover:not(:disabled) {
  background: var(--color-bg-hover);
  color: var(--color-danger, #c0392b);
}

.queue-text-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.queue-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
}

.queue-close svg {
  width: 15px;
  height: 15px;
}

.queue-close:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.queue-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--space-2);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.queue-empty {
  margin: var(--space-4);
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-align: center;
  line-height: 1.6;
}

.queue-row {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  border-radius: var(--radius-md);
  padding-right: 4px;
}

.queue-row:hover {
  background: var(--color-bg-hover);
}

.queue-row.active {
  background: var(--color-primary-bg);
}

.queue-main {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 7px 8px;
  text-align: left;
}

.queue-index {
  width: 18px;
  flex-shrink: 0;
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  text-align: right;
}

.queue-row.active .queue-index {
  color: var(--color-primary);
}

.queue-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.queue-text strong,
.queue-text small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.queue-text strong {
  color: var(--color-text);
  font-size: 13px;
}

.queue-row.active .queue-text strong {
  color: var(--color-primary);
  font-weight: 600;
}

.queue-text small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.queue-duration {
  flex-shrink: 0;
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.queue-remove {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  opacity: 0;
}

.queue-row:hover .queue-remove,
.queue-remove:focus-visible {
  opacity: 1;
}

.queue-remove:hover {
  color: var(--color-danger, #c0392b);
}

.queue-remove svg {
  width: 13px;
  height: 13px;
}
</style>
