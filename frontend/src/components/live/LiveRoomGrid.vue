<template>
  <div class="live-grid">
    <button
      v-for="room in rooms"
      :key="`${room.platform}:${room.roomId}`"
      class="room-card"
      @click="$emit('select', room)"
    >
      <div class="room-cover">
        <img :src="room.cover" :alt="room.title" loading="lazy" referrerpolicy="no-referrer" />
        <span class="room-online">{{ formatOnline(room.online) }}</span>
      </div>
      <div class="room-meta">
        <strong :title="room.title">{{ room.title }}</strong>
        <small>{{ room.userName }}<template v-if="room.areaName"> · {{ room.areaName }}</template></small>
      </div>
    </button>
  </div>
</template>

<script setup lang="ts">
import type { LiveRoomSummary } from '../../api/live'

defineProps<{ rooms: LiveRoomSummary[] }>()
defineEmits<{ select: [room: LiveRoomSummary] }>()

function formatOnline(count: number) {
  if (count >= 10000) return `${(count / 10000).toFixed(1)}万`
  return String(count)
}
</script>

<style scoped>
.live-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(232px, 1fr));
  gap: 16px;
}

.room-card {
  display: flex;
  flex-direction: column;
  gap: 9px;
  padding: 0;
  border: none;
  background: transparent;
  text-align: left;
}

.room-cover {
  position: relative;
  aspect-ratio: 16 / 9;
  overflow: hidden;
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-sunken);
}

.room-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: transform var(--duration-normal) var(--ease-out);
}

.room-card:hover .room-cover img {
  transform: scale(1.03);
}

.room-online {
  position: absolute;
  right: 8px;
  bottom: 8px;
  padding: 2px 7px;
  border-radius: var(--radius-sm);
  background: rgba(0, 0, 0, 0.55);
  color: #fff;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  backdrop-filter: blur(6px);
}

.room-meta {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.room-meta strong {
  overflow: hidden;
  color: var(--color-text);
  font-size: 14px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.room-meta small {
  overflow: hidden;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (max-width: 640px) {
  .live-grid {
    grid-template-columns: repeat(auto-fill, minmax(158px, 1fr));
    gap: 12px;
  }
}
</style>
