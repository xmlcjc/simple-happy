<template>
  <div class="music-pane">
    <form class="search-row" role="search" @submit.prevent="store.search()">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
        <circle cx="11" cy="11" r="8" />
        <path d="m21 21-4.3-4.3" />
      </svg>
      <input
        v-model="store.keyword"
        type="search"
        placeholder="搜索歌曲、歌手"
        aria-label="搜索音乐"
      />
      <button type="submit" class="search-submit" :disabled="store.searching || !store.keyword.trim()">
        {{ store.searching ? '搜索中…' : '搜索' }}
      </button>
    </form>

    <div v-if="store.enabledPlugins.length" class="platform-chips">
      <button
        v-for="plugin in store.enabledPlugins"
        :key="plugin.platform"
        type="button"
        class="platform-chip"
        :class="{ active: store.activePlatform === plugin.platform }"
        :title="plugin.name"
        @click="store.setActivePlatform(plugin.platform)"
      >
        {{ plugin.name }}
      </button>
    </div>

    <div v-if="!store.enabledPlugins.length" class="music-state">
      <p>还没有启用的音乐插件</p>
      <small>音乐功能本身不内置音源，安装 MusicFree 插件后即可搜索与播放。</small>
      <button type="button" class="state-btn" @click="emit('open-plugins')">去安装插件</button>
    </div>

    <div v-else-if="store.searchError" class="music-state">
      <p>{{ store.searchError }}</p>
    </div>

    <div v-else-if="!store.results.length && !store.searching" class="music-state">
      <p>{{ store.keyword.trim() ? '没有找到相关歌曲' : '输入关键词开始搜索' }}</p>
      <small>搜索由已启用的插件提供，结果质量取决于插件本身。</small>
    </div>

    <template v-else>
      <MusicTrackList :items="store.results" @play="emit('play', $event, store.results)">
        <template #actions="{ item }">
          <button
            type="button"
            class="row-action"
            :class="{ active: libraryStore.isFavorite(item) }"
            title="收藏"
            :aria-label="libraryStore.isFavorite(item) ? '取消收藏' : '收藏'"
            @click.stop="toggleFavorite(item)"
          >
            <svg viewBox="0 0 24 24" :fill="libraryStore.isFavorite(item) ? 'currentColor' : 'none'" stroke="currentColor" stroke-width="2">
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

      <div v-if="store.hasMore" class="more-row">
        <button type="button" class="state-btn" :disabled="store.searching" @click="store.loadMore()">
          {{ store.searching ? '加载中…' : '加载更多' }}
        </button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { useMusicStore } from '../../stores/music'
import { useMusicLibraryStore } from '../../stores/musicLibrary'
import { useAppStore } from '../../stores/app'
import MusicTrackList from './MusicTrackList.vue'
import type { MusicItem } from '../../api/music'

const emit = defineEmits<{
  'open-plugins': []
  play: [item: MusicItem, queue: MusicItem[]]
  'add-to-playlist': [item: MusicItem]
}>()

const store = useMusicStore()
const libraryStore = useMusicLibraryStore()
const appStore = useAppStore()

async function toggleFavorite(item: MusicItem) {
  try {
    const favorited = await libraryStore.toggleFavorite(item)
    appStore.showToast(favorited ? '已加入收藏' : '已取消收藏', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '收藏失败', 'error')
  }
}
</script>

<style scoped>
.music-pane {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  overflow: hidden;
}

.search-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 6px 8px 6px var(--space-4);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
}

.search-row svg {
  width: 17px;
  height: 17px;
  flex-shrink: 0;
  color: var(--color-text-tertiary);
}

.search-row input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: none;
  font-size: var(--text-sm);
  color: var(--color-text);
}

.search-row input::placeholder {
  color: var(--color-text-tertiary);
}

.search-submit {
  flex-shrink: 0;
  padding: 7px 16px;
  border-radius: var(--radius-md);
  background: var(--color-primary-soft-bg);
  color: var(--color-primary-soft-text);
  font-size: var(--text-sm);
  font-weight: 600;
}

.search-submit:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.platform-chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.platform-chip {
  padding: 5px 12px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.platform-chip.active {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
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

.more-row {
  display: flex;
  justify-content: center;
  padding: var(--space-3) 0;
}

.music-state {
  padding: 64px 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  color: var(--color-text-tertiary);
  font-size: 13px;
  text-align: center;
}

.music-state p {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: 14px;
}

.state-btn {
  padding: 7px 16px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border);
  color: var(--color-text-secondary);
  font-size: 13px;
}

.state-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}
</style>
