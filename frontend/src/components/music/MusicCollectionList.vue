<template>
  <div v-if="!items.length" class="collection-empty">
    <p>{{ emptyText }}</p>
  </div>
  <div v-else class="collection-grid">
    <button
      v-for="item in items"
      :key="`${item.platform}:${item.id}`"
      type="button"
      class="collection-card"
      :title="item.title"
      @click="emit('open', item)"
    >
      <span class="collection-cover">
        <img
          v-if="item.artwork"
          :src="item.artwork"
          :alt="item.title"
          loading="lazy"
          referrerpolicy="no-referrer"
        />
        <span v-else class="cover-fallback">{{ (item.title || '?').slice(0, 1) }}</span>
      </span>
      <strong>{{ item.title || '未知名称' }}</strong>
      <small>{{ subtitle(item) }}</small>
    </button>
  </div>
</template>

<script setup lang="ts">
import type { MusicCollectionItem } from '../../api/music'

withDefaults(
  defineProps<{
    items: MusicCollectionItem[]
    emptyText?: string
  }>(),
  { emptyText: '没有找到相关内容' },
)

const emit = defineEmits<{
  open: [item: MusicCollectionItem]
}>()

function subtitle(item: MusicCollectionItem) {
  const parts: string[] = []
  if (item.artist) parts.push(item.artist)
  if (item.worksNum) parts.push(`${Math.round(item.worksNum)} 首`)
  if (item.playCount) parts.push(`${formatCount(item.playCount)} 播放`)
  return parts.join(' · ') || item.date || ' '
}

function formatCount(value: number) {
  if (value >= 100_000_000) return `${(value / 100_000_000).toFixed(1)} 亿`
  if (value >= 10_000) return `${(value / 10_000).toFixed(1)} 万`
  return String(Math.round(value))
}
</script>

<style scoped>
.collection-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
  gap: var(--space-3);
  overflow-y: auto;
  min-height: 0;
  padding-bottom: var(--space-2);
}

.collection-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
  border-radius: var(--radius-md);
  text-align: left;
  transition: background var(--duration-fast) var(--ease-out);
}

.collection-card:hover {
  background: var(--color-bg-hover);
}

.collection-cover {
  position: relative;
  width: 100%;
  aspect-ratio: 1;
  border-radius: var(--radius-md);
  overflow: hidden;
  background: var(--color-bg-sunken);
}

.collection-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.cover-fallback {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
  font-size: 28px;
  font-weight: 600;
}

.collection-card strong {
  color: var(--color-text);
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.collection-card small {
  color: var(--color-text-tertiary);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.collection-empty {
  padding: 56px 20px;
  color: var(--color-text-tertiary);
  font-size: 13px;
  text-align: center;
}

.collection-empty p {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: 14px;
}
</style>
