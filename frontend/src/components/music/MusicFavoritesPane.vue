<template>
  <div class="music-pane">
    <div class="pane-toolbar">
      <span class="pane-count">{{ libraryStore.favorites.length }} 首收藏</span>
      <button type="button" class="ghost-btn" @click="refresh">刷新</button>
    </div>

    <MusicTrackList
      :items="libraryStore.favorites"
      empty-text="还没有收藏的歌曲"
      empty-hint="在搜索结果里点心形按钮即可收藏。"
      @play="emit('play', $event, libraryStore.favorites)"
    >
      <template #actions="{ item }">
        <button
          type="button"
          class="row-action active"
          title="取消收藏"
          @click.stop="removeFavorite(item)"
        >
          <svg viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2">
            <path d="M12 20s-7-4.4-7-9.2A4 4 0 0 1 12 8a4 4 0 0 1 7 2.8C19 15.6 12 20 12 20z" />
          </svg>
        </button>
        <button type="button" class="row-action" title="加入歌单" @click.stop="emit('add-to-playlist', item)">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
      </template>
    </MusicTrackList>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useMusicLibraryStore } from '../../stores/musicLibrary'
import { useAppStore } from '../../stores/app'
import MusicTrackList from './MusicTrackList.vue'
import type { MusicItem } from '../../api/music'

const emit = defineEmits<{
  play: [item: MusicItem, queue: MusicItem[]]
  'add-to-playlist': [item: MusicItem]
}>()

const libraryStore = useMusicLibraryStore()
const appStore = useAppStore()

async function refresh() {
  try {
    await libraryStore.loadFavorites()
  } catch (error) {
    appStore.showToast((error as Error).message || '加载收藏失败', 'error')
  }
}

async function removeFavorite(item: MusicItem) {
  try {
    await libraryStore.toggleFavorite(item)
    appStore.showToast('已取消收藏', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '操作失败', 'error')
  }
}

onMounted(refresh)
</script>

<style scoped>
.music-pane {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  overflow: hidden;
}

.pane-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.pane-count {
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.ghost-btn {
  padding: 5px 12px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.ghost-btn:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.row-action {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
}

.row-action svg {
  width: 15px;
  height: 15px;
}

.row-action:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.row-action.active {
  color: var(--color-primary);
}
</style>
