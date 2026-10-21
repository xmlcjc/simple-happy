<template>
  <div class="music-view">
    <aside class="music-rail" aria-label="音乐分类">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        type="button"
        class="rail-item"
        :class="{ active: activeTab === tab.key }"
        :title="tab.label"
        @click="selectTab(tab.key)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path v-for="(path, index) in tab.paths" :key="index" :d="path" />
        </svg>
        <span>{{ tab.label }}</span>
      </button>
    </aside>

    <section class="music-main">
      <header class="music-head">
        <h1>{{ activeTitle }}</h1>
        <div class="music-head-meta">
          <span v-if="store.enabledPlugins.length" class="plugin-count">
            {{ store.enabledPlugins.length }} 个音源已启用
          </span>
          <button type="button" class="head-btn" title="刷新插件" @click="store.loadPlugins()">
            刷新
          </button>
        </div>
      </header>

      <div class="music-body">
        <MusicSearchPane
          v-if="activeTab === 'search'"
          @open-plugins="activeTab = 'plugins'"
          @play="handlePlay"
          @play-next="playNext"
          @add-to-playlist="openPicker"
          @open-collection="openCollection"
        />
        <MusicFavoritesPane
          v-else-if="activeTab === 'favorites'"
          @play="handlePlay"
          @play-next="playNext"
          @add-to-playlist="openPicker"
        />
        <MusicDiscoverPane
          v-else-if="activeTab === 'discover'"
          @open="openCollection"
        />
        <MusicPlaylistsPane
          v-else-if="activeTab === 'playlists'"
          @play="handlePlay"
          @play-next="playNext"
          @import-items="openPickerForItems"
        />
        <MusicLocalPane
          v-else-if="activeTab === 'local'"
          @play="handlePlay"
          @play-next="playNext"
        />
        <MusicHistoryPane v-else-if="activeTab === 'history'" @play="handlePlay" />
        <MusicPluginPane v-else-if="activeTab === 'plugins'" />

        <div v-else class="music-state">
          <p>{{ activeTitle }}功能即将上线</p>
        </div>

        <MusicLyricPanel
          v-if="lyricOpen && current"
          :lines="lyricLines"
          :current-index="lyricIndex"
          :loading="lyricLoading"
          :error="lyricError"
          :offset-ms="settings.lyricOffsetMs"
          :font-size-px="lyricFontPx"
          :font-label="lyricFontLabel"
          @close="lyricOpen = false"
          @seek="seek"
          @shift-offset="shiftLyricOffset"
          @reset-offset="settings.setLyricOffsetMs(0)"
          @cycle-font-size="cycleLyricFontSize"
        />

        <MusicQueuePanel
          v-if="queueOpen"
          :items="playerQueue"
          :current-index="queueIndex"
          @close="queueOpen = false"
          @jump="jumpTo"
          @remove="removeAt"
          @clear="clearQueue"
        />

        <MusicDetailPanel
          v-if="detail"
          :platform="detail.item.platform"
          :kind="detail.kind"
          :item="detail.item"
          @close="detail = null"
          @play="handlePlay"
          @play-next="playNext"
          @add-to-playlist="openPicker"
          @open-collection="openCollection"
        />
      </div>

      <MusicPlayerBar
        :current="current"
        :playing="playing"
        :loading="playerLoading"
        :current-time="currentTime"
        :duration="duration"
        :volume="volume"
        :error-text="playerError"
        :lyric-open="lyricOpen"
        :play-mode="settings.playMode"
        :playback-rate="settings.playbackRate"
        :queue-count="playerQueue.length"
        :queue-open="queueOpen"
        :quality-label="qualityLabel"
        :preferred-quality="settings.preferredQuality"
        :quality-order="settings.qualityOrder"
        :swapped-from="swappedFrom"
        :sleep-mode="sleepMode"
        :sleep-remaining="sleepRemaining"
        @toggle="toggle"
        @next="next"
        @prev="prev"
        @seek="seek"
        @volume="setVolume"
        @toggle-lyric="toggleLyric"
        @toggle-queue="toggleQueue"
        @cycle-mode="settings.cyclePlayMode()"
        @cycle-rate="settings.setPlaybackRate(nextPlaybackRate(settings.playbackRate))"
        @set-quality="changeQuality"
        @set-quality-order="settings.setQualityOrder"
        @set-sleep="setSleepCountdown"
        @sleep-end="setSleepEndOfTrack"
        @cancel-sleep="cancelSleep"
      />

      <MusicPlaylistPicker v-model="pickerOpen" :items="pickerItems" />
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useMusicStore } from '../stores/music'
import { useMusicLibraryStore } from '../stores/musicLibrary'
import { nextPlaybackRate, MUSIC_QUALITY_OPTIONS, LYRIC_FONT_SIZE_OPTIONS, useMusicSettingsStore, type MusicQuality } from '../stores/musicSettings'
import { useMusicPlayer } from '../composables/useMusicPlayer'
import { useMusicLyric } from '../composables/useMusicLyric'
import MusicSearchPane from '../components/music/MusicSearchPane.vue'
import MusicDiscoverPane from '../components/music/MusicDiscoverPane.vue'
import MusicFavoritesPane from '../components/music/MusicFavoritesPane.vue'
import MusicPlaylistsPane from '../components/music/MusicPlaylistsPane.vue'
import MusicHistoryPane from '../components/music/MusicHistoryPane.vue'
import MusicLocalPane from '../components/music/MusicLocalPane.vue'
import MusicPluginPane from '../components/music/MusicPluginPane.vue'
import MusicPlayerBar from '../components/music/MusicPlayerBar.vue'
import MusicLyricPanel from '../components/music/MusicLyricPanel.vue'
import MusicQueuePanel from '../components/music/MusicQueuePanel.vue'
import MusicDetailPanel from '../components/music/MusicDetailPanel.vue'
import MusicPlaylistPicker from '../components/music/MusicPlaylistPicker.vue'
import type {
  MusicCollectionItem,
  MusicCollectionKind,
  MusicItem,
} from '../api/music'

type MusicTab = 'search' | 'discover' | 'favorites' | 'playlists' | 'local' | 'history' | 'plugins'

const store = useMusicStore()
const libraryStore = useMusicLibraryStore()
const settings = useMusicSettingsStore()
const activeTab = ref<MusicTab>('search')
const lyricOpen = ref(false)
const queueOpen = ref(false)
const pickerOpen = ref(false)
const pickerItems = ref<MusicItem[]>([])
const detail = ref<{ item: MusicCollectionItem; kind: MusicCollectionKind } | null>(null)

const {
  current,
  playing,
  loading: playerLoading,
  currentTime,
  duration,
  volume,
  errorText: playerError,
  effectiveQuality,
  swappedFrom,
  sleepMode,
  sleepRemaining,
  queue: playerQueue,
  playQueue,
  index: queueIndex,
  playNext,
  jumpTo,
  removeAt,
  clearQueue,
  reload,
  setSleepCountdown,
  setSleepEndOfTrack,
  cancelSleep,
  toggle,
  next,
  prev,
  seek,
  setVolume,
} = useMusicPlayer()

const {
  lines: lyricLines,
  currentIndex: lyricIndex,
  loading: lyricLoading,
  error: lyricError,
} = useMusicLyric(current, currentTime)

const tabs: Array<{ key: MusicTab; label: string; paths: string[] }> = [
  {
    key: 'search',
    label: '搜索',
    paths: ['M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16z', 'm21 21-4.3-4.3'],
  },
  {
    key: 'discover',
    label: '发现',
    paths: [
      'M12 3l2.3 5.2L20 10.5l-5.7 2.3L12 18l-2.3-5.2L4 10.5l5.7-2.3z',
    ],
  },
  {
    key: 'favorites',
    label: '收藏',
    paths: ['M12 20s-7-4.4-7-9.2A4 4 0 0 1 12 8a4 4 0 0 1 7 2.8C19 15.6 12 20 12 20z'],
  },
  {
    key: 'playlists',
    label: '歌单',
    paths: ['M4 6h10', 'M4 12h10', 'M4 18h6', 'M18 15v5', 'M20.5 17.5h-5'],
  },
  {
    key: 'local',
    label: '本地',
    paths: ['M4 7h5l2 2h9v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V7z'],
  },
  {
    key: 'history',
    label: '历史',
    paths: ['M12 7v5l3 2', 'M12 3a9 9 0 1 0 9 9'],
  },
  {
    key: 'plugins',
    label: '插件',
    paths: [
      'M10 4h4v3a2 2 0 1 0 4 0h2v6h-3a2 2 0 1 0 0 4v3H4v-5h3a2 2 0 1 0 0-4H4V6h6z',
    ],
  },
]

const activeTitle = computed(() => tabs.find((tab) => tab.key === activeTab.value)?.label || '音乐')

/** 播放条显示的实际音质：插件可能返回 320k 这类自定义值，不在选项里就原样显示。 */
const qualityLabel = computed(() => {
  const value = effectiveQuality.value || settings.preferredQuality
  return MUSIC_QUALITY_OPTIONS.find((option) => option.value === value)?.label || value
})

const lyricFontOption = computed(
  () =>
    LYRIC_FONT_SIZE_OPTIONS.find((option) => option.value === settings.lyricFontSize)
    ?? LYRIC_FONT_SIZE_OPTIONS[1],
)
const lyricFontPx = computed(() => lyricFontOption.value.px)
const lyricFontLabel = computed(() => lyricFontOption.value.label)

function shiftLyricOffset(deltaMs: number) {
  settings.setLyricOffsetMs(settings.lyricOffsetMs + deltaMs)
}

function cycleLyricFontSize() {
  const index = LYRIC_FONT_SIZE_OPTIONS.findIndex(
    (option) => option.value === settings.lyricFontSize,
  )
  const next = LYRIC_FONT_SIZE_OPTIONS[(index + 1) % LYRIC_FONT_SIZE_OPTIONS.length]
  settings.setLyricFontSize(next.value)
}

function handlePlay(item: MusicItem, queue?: MusicItem[]) {
  // 优先使用来源面板给出的列表; 搜索结果作为默认队列。
  const list = queue && queue.length ? queue : store.results
  const start = list.findIndex(
    (entry) => entry.platform === item.platform && entry.id === item.id,
  )
  void playQueue(start < 0 ? [item] : list, start < 0 ? 0 : start)
}

function openPicker(item: MusicItem) {
  pickerItems.value = [item]
  pickerOpen.value = true
}

/** 链接导入解析出多首时，直接复用歌单选择器。 */
function openPickerForItems(items: MusicItem[]) {
  if (!items.length) return
  pickerItems.value = [...items]
  pickerOpen.value = true
}

// 歌词与队列共用右侧浮层，同时只开一个。
function toggleLyric() {
  lyricOpen.value = !lyricOpen.value
  if (lyricOpen.value) queueOpen.value = false
}

function toggleQueue() {
  queueOpen.value = !queueOpen.value
  if (queueOpen.value) lyricOpen.value = false
}

/** 打开专辑/歌手/歌单详情，覆盖内容区。 */
function openCollection(item: MusicCollectionItem, kind: MusicCollectionKind) {
  detail.value = { item, kind }
  lyricOpen.value = false
  queueOpen.value = false
}

/** 切 tab 时收起详情，避免详情页浮在别的分类上。 */
function selectTab(key: MusicTab) {
  activeTab.value = key
  detail.value = null
}

/** 切音质：记住偏好并立刻用新音质重新取流。 */
function changeQuality(quality: MusicQuality) {
  settings.setPreferredQuality(quality)
  if (current.value) reload()
}

onMounted(() => {
  void store.loadPlugins()
  void libraryStore.loadFavorites().catch(() => undefined)
})
</script>

<style scoped>
.music-view {
  height: 100%;
  min-height: 0;
  overflow: hidden;
  display: flex;
  /* 功能栏贴窗口左缘, 内容列吃满剩余宽度。 */
  padding: 0 var(--space-6) var(--space-4) 0;
}

.music-rail {
  flex-shrink: 0;
  width: 56px;
  padding: var(--space-3) 4px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  border-right: 1px solid var(--color-divider);
  overflow-y: auto;
  scrollbar-width: none;
}

.music-rail::-webkit-scrollbar {
  display: none;
}

.rail-item {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  gap: 4px;
  padding: 8px 2px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 11px;
  transition: all var(--duration-fast) var(--ease-out);
}

.rail-item svg {
  width: 20px;
  height: 20px;
  flex-shrink: 0;
}

.rail-item:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.rail-item.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.music-main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding-left: var(--space-4);
}

.music-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-5) 0 var(--space-3);
  flex-shrink: 0;
}

.music-head h1 {
  margin: 0;
  font-size: var(--text-xl);
  font-weight: 700;
  color: var(--color-text);
}

.music-head-meta {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.plugin-count {
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.head-btn {
  padding: 5px 12px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.head-btn:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.music-body {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
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

@media (max-width: 900px) {
  .music-view {
    padding: 0 var(--space-3) var(--space-3) 0;
  }

  .music-rail {
    width: 48px;
    padding: var(--space-2) 2px;
  }

  .rail-item {
    font-size: 10px;
    padding: 7px 1px;
  }

  .rail-item svg {
    width: 18px;
    height: 18px;
  }

  .music-main {
    padding-left: var(--space-3);
  }
}
</style>
