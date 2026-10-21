<template>
  <div v-if="!items.length" class="track-empty">
    <p>{{ emptyText }}</p>
    <small v-if="emptyHint">{{ emptyHint }}</small>
  </div>
  <div v-else class="track-list">
    <button
      v-for="item in items"
      :key="`${item.platform}:${item.id}`"
      type="button"
      class="track-row"
      @click="emit('play', item)"
    >
      <img
        v-if="item.artwork"
        class="track-cover"
        :src="item.artwork"
        :alt="item.title"
        loading="lazy"
        referrerpolicy="no-referrer"
      />
      <span v-else class="track-cover placeholder">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
          <path d="M9 18V6l10-2v12" />
          <circle cx="6" cy="18" r="3" />
          <circle cx="16" cy="16" r="3" />
        </svg>
      </span>
      <span class="track-text">
        <strong>{{ item.title || '未知标题' }}</strong>
        <small>{{ subtitle(item) }}</small>
      </span>
      <span v-if="item.duration" class="track-duration">{{ formatDuration(item.duration) }}</span>
      <slot name="actions" :item="item"></slot>
      <span v-if="showSource" class="track-source">{{ item.platform }}</span>
    </button>
  </div>
</template>

<script setup lang="ts">
import type { MusicItem } from '../../api/music'

withDefaults(
  defineProps<{
    items: MusicItem[]
    emptyText?: string
    emptyHint?: string
    showSource?: boolean
  }>(),
  {
    emptyText: '暂无歌曲',
    emptyHint: '',
    showSource: true,
  },
)

const emit = defineEmits<{
  play: [item: MusicItem]
}>()

function subtitle(item: MusicItem) {
  return [item.artist, item.album].filter(Boolean).join(' · ') || '未知歌手'
}

function formatDuration(seconds: number) {
  const total = Math.max(0, Math.round(seconds))
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`
}
</script>

<style scoped>
.track-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow-y: auto;
  min-height: 0;
}

.track-row {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 8px 10px;
  border-radius: var(--radius-md);
  text-align: left;
  transition: background var(--duration-fast) var(--ease-out);
}

.track-row:hover {
  background: var(--color-bg-hover);
}

.track-cover {
  width: 40px;
  height: 40px;
  border-radius: var(--radius-sm);
  object-fit: cover;
  flex-shrink: 0;
  background: var(--color-bg-sunken);
}

.track-cover.placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
}

.track-cover.placeholder svg {
  width: 20px;
  height: 20px;
}

.track-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.track-text strong,
.track-text small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.track-text strong {
  color: var(--color-text);
  font-size: 14px;
  font-weight: 600;
}

.track-text small {
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.track-duration {
  flex-shrink: 0;
  color: var(--color-text-tertiary);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.track-source {
  flex-shrink: 0;
  padding: 2px 8px;
  border-radius: var(--radius-sm);
  background: var(--color-bg-sunken);
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.track-empty {
  padding: 56px 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  color: var(--color-text-tertiary);
  font-size: 13px;
  text-align: center;
}

.track-empty p {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: 14px;
}
</style>
