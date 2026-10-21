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
        :placeholder="placeholder"
        aria-label="搜索音乐"
      />
      <button type="submit" class="search-submit" :disabled="store.searching || !store.keyword.trim()">
        {{ store.searching ? '搜索中…' : '搜索' }}
      </button>
    </form>

    <div class="type-chips" role="tablist" aria-label="搜索类型">
      <button
        v-for="type in SEARCH_TYPES"
        :key="type.value"
        type="button"
        role="tab"
        class="type-chip"
        :class="{ active: store.searchType === type.value }"
        :aria-selected="store.searchType === type.value"
        @click="store.setSearchType(type.value)"
      >{{ type.label }}</button>
    </div>

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

    <div v-else-if="!hasItems && !store.searching" class="music-state">
      <p>{{ store.keyword.trim() ? emptyText : '输入关键词开始搜索' }}</p>
      <small>搜索由已启用的插件提供，结果质量取决于插件本身。</small>

      <div v-if="store.searchHistory.length" class="history-block">
        <div class="history-head">
          <span>最近搜索</span>
          <button type="button" @click="store.clearSearchHistory()">清空</button>
        </div>
        <div class="history-chips">
          <span v-for="word in store.searchHistory" :key="word" class="history-chip">
            <button type="button" class="history-word" @click="searchAgain(word)">{{ word }}</button>
            <button
              type="button"
              class="history-remove"
              :aria-label="`删除搜索记录 ${word}`"
              title="删除"
              @click="store.removeSearchHistory(word)"
            >×</button>
          </span>
        </div>
      </div>
    </div>

    <template v-else>
      <MusicTrackList
        v-if="store.searchType === 'music'"
        :items="store.results"
        @play="emit('play', $event, store.results)"
        @play-next="emit('play-next', $event)"
      >
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

      <MusicCollectionList
        v-else
        :items="store.collections"
        :empty-text="emptyText"
        @open="emit('open-collection', $event, store.searchType === 'artist' ? 'artist' : 'album')"
      ></MusicCollectionList>

      <div v-if="store.hasMore" class="more-row">
        <button type="button" class="state-btn" :disabled="store.searching" @click="store.loadMore()">
          {{ store.searching ? '加载中…' : '加载更多' }}
        </button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useMusicStore } from '../../stores/music'
import { useMusicLibraryStore } from '../../stores/musicLibrary'
import { useAppStore } from '../../stores/app'
import MusicTrackList from './MusicTrackList.vue'
import MusicCollectionList from './MusicCollectionList.vue'
import type {
  MusicCollectionItem,
  MusicCollectionKind,
  MusicItem,
  MusicSearchType,
} from '../../api/music'

const emit = defineEmits<{
  'open-plugins': []
  play: [item: MusicItem, queue: MusicItem[]]
  'play-next': [item: MusicItem]
  'add-to-playlist': [item: MusicItem]
  'open-collection': [item: MusicCollectionItem, kind: MusicCollectionKind]
}>()

const SEARCH_TYPES: Array<{ value: MusicSearchType; label: string }> = [
  { value: 'music', label: '单曲' },
  { value: 'album', label: '专辑' },
  { value: 'artist', label: '歌手' },
]

const store = useMusicStore()
const libraryStore = useMusicLibraryStore()
const appStore = useAppStore()

const hasItems = computed(() =>
  store.searchType === 'music' ? store.results.length > 0 : store.collections.length > 0,
)

const placeholder = computed(
  () =>
    ({
      music: '搜索歌曲、歌手',
      album: '搜索专辑',
      artist: '搜索歌手',
      sheet: '搜索歌单',
    })[store.searchType],
)

const emptyText = computed(() =>
  store.searchType === 'music' ? '没有找到相关歌曲' : '没有找到相关内容',
)

function searchAgain(word: string) {
  store.keyword = word
  void store.search()
}

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

.history-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: var(--space-3);
  width: min(420px, 100%);
}

.history-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.history-head button {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.history-head button:hover {
  color: var(--color-danger, #c0392b);
}

.history-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  justify-content: center;
}

.history-chip {
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-full);
  overflow: hidden;
}

.history-word {
  padding: 4px 4px 4px 12px;
  color: var(--color-text-secondary);
  font-size: 12px;
}

.history-word:hover {
  color: var(--color-primary);
}

.history-remove {
  padding: 4px 9px 4px 4px;
  color: var(--color-text-tertiary);
  font-size: 13px;
  line-height: 1;
}

.history-remove:hover {
  color: var(--color-danger, #c0392b);
}

.type-chips {
  display: flex;
  gap: 2px;
  padding: 3px;
  align-self: flex-start;
  border-radius: var(--radius-full);
  background: var(--color-bg-sunken);
}

.type-chip {
  padding: 4px 14px;
  border-radius: var(--radius-full);
  color: var(--color-text-tertiary);
  font-size: 12px;
  transition: all var(--duration-fast) var(--ease-out);
}

.type-chip:hover {
  color: var(--color-text);
}

.type-chip.active {
  background: var(--color-bg-elevated);
  color: var(--color-primary);
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
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
