<template>
  <div class="live-view">
    <nav class="live-rail" aria-label="直播导航">
      <button
        v-for="item in railItems"
        :key="item.key"
        class="rail-item"
        :class="{ active: store.view === item.key }"
        :title="item.label"
        @click="store.setView(item.key)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path v-for="(path, index) in item.paths" :key="index" :d="path" />
        </svg>
        <span>{{ item.label }}</span>
      </button>
    </nav>

    <div class="live-content">
      <header class="live-header">
        <div class="live-title-row">
          <h1 class="live-title">{{ activeTitle }}</h1>
        </div>
        <div class="live-actions">
          <div class="live-search" :class="{ focused: searchFocused }">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="11" cy="11" r="7" />
              <path d="M20 20l-3.5-3.5" />
            </svg>
            <input
              v-model="store.keyword"
              type="search"
              placeholder="搜索直播间 / 主播"
              @focus="searchFocused = true"
              @blur="searchFocused = false"
              @keyup.enter="store.runSearch()"
            />
            <button v-if="store.keyword" class="search-clear" title="清空" @click="store.clearSearch()">×</button>
          </div>
          <button class="refresh-btn" :disabled="store.loading" @click="reload">刷新</button>
        </div>
      </header>

      <div class="live-platforms">
        <button
          v-for="item in store.platforms"
          :key="item.id"
          class="platform-tab"
          :class="{ active: store.platform === item.id }"
          @click="store.switchPlatform(item.id)"
        >
          {{ item.name }}
        </button>
      </div>

      <div v-if="store.error" class="live-state">
        <p>{{ store.error }}</p>
        <button class="state-btn" @click="reload">重试</button>
      </div>

      <!-- 搜索结果优先展示 -->
      <template v-else-if="store.keyword.trim()">
        <div v-if="store.searching" class="live-state"><p>正在搜索…</p></div>
        <div v-else-if="!store.searchResults.length" class="live-state">
          <p>没有找到「{{ store.keyword }}」相关的直播间</p>
        </div>
        <LiveRoomGrid v-else :rooms="store.searchResults" @select="openRoom" />
      </template>

      <template v-else-if="store.view === 'follow'">
        <div v-if="!store.follows.length" class="live-state">
          <p>还没有关注的直播间</p>
          <small>在直播间页点「关注」即可加入这里</small>
        </div>
        <div v-else class="follow-list">
          <button
            v-for="item in sortedFollows"
            :key="`${item.platform}:${item.roomId}`"
            class="follow-row"
            @click="openFollow(item)"
          >
            <img
              v-if="item.avatar"
              class="follow-avatar"
              :src="item.avatar"
              :alt="item.anchorName"
              referrerpolicy="no-referrer"
            />
            <span v-else class="follow-avatar placeholder">{{ (item.anchorName || '?').slice(0, 1) }}</span>
            <span class="follow-text">
              <strong>{{ item.anchorName || `房间 ${item.roomId}` }}</strong>
              <small>{{ item.roomName }}</small>
            </span>
            <span
              v-if="followLiveState(item) !== undefined"
              class="follow-status"
              :class="{ live: followLiveState(item) }"
            >{{ followLiveState(item) ? '直播中' : '未开播' }}</span>
            <span class="follow-platform">{{ platformName(item.platform) }}</span>
          </button>
        </div>
      </template>

      <template v-else-if="store.view === 'history'">
        <div v-if="!store.history.length" class="live-state">
          <p>还没有观看记录</p>
        </div>
        <div v-else class="follow-list">
          <button
            v-for="item in store.history"
            :key="`${item.platform}:${item.roomId}`"
            class="follow-row"
            @click="openHistory(item)"
          >
            <span class="follow-avatar placeholder">{{ (item.anchorName || '?').slice(0, 1) }}</span>
            <span class="follow-text">
              <strong>{{ item.roomName || item.anchorName || `房间 ${item.roomId}` }}</strong>
              <small>{{ item.anchorName }} · 最近 {{ item.lastDate }}</small>
            </span>
            <span class="follow-platform">{{ platformName(item.platform) }}</span>
          </button>
        </div>
      </template>

      <template v-else-if="store.view === 'category'">
        <div v-if="store.loading" class="live-state"><p>正在加载分区…</p></div>
        <div v-else class="category-groups">
          <section v-for="group in store.categories" :key="group.id" class="category-group">
            <h2>{{ group.name }}</h2>
            <div class="category-grid">
              <button
                v-for="area in visibleAreas(group)"
                :key="area.id"
                class="category-tile"
                :title="area.name"
                @click="openArea(group.id, area.id)"
              >
                <span class="category-icon">
                  <img
                    v-if="area.icon"
                    :src="area.icon"
                    :alt="area.name"
                    loading="lazy"
                    referrerpolicy="no-referrer"
                  />
                  <span v-else class="category-icon-fallback">{{ area.name.slice(0, 1) }}</span>
                </span>
                <span class="category-name">{{ area.name }}</span>
              </button>
            </div>
            <button
              v-if="group.children.length > CATEGORY_PREVIEW"
              class="show-all"
              @click="toggleGroup(group.id)"
            >
              {{ expandedGroups[group.id] ? '收起' : '显示全部' }}
            </button>
          </section>
        </div>
      </template>

      <template v-else>
        <!-- 分区选择统一在左侧「分类」页完成，首页只保留一个可清除的当前筛选 -->
        <div v-if="currentArea" class="live-filter">
          <span class="filter-chip">
            {{ currentArea }}
            <button class="filter-clear" title="清除筛选" @click="clearArea">×</button>
          </span>
        </div>

        <div v-if="store.loading && !store.rooms.length" class="live-state">
          <p>正在加载直播间…</p>
        </div>
        <div v-else-if="!store.rooms.length" class="live-state">
          <p>当前分区没有正在直播的房间</p>
        </div>
        <template v-else>
          <LiveRoomGrid :rooms="store.rooms" @select="openRoom" />
          <div v-if="store.hasMore" class="live-more">
            <button class="state-btn" :disabled="store.loadingMore" @click="store.loadMore()">
              {{ store.loadingMore ? '加载中…' : '加载更多' }}
            </button>
          </div>
        </template>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useLiveStore, type LiveView } from '../stores/live'
import LiveRoomGrid from '../components/live/LiveRoomGrid.vue'
import type { LiveFollow } from '../api/liveFollow'
import type { LiveRoomStats } from '../api/liveStats'
import type { LiveRoomSummary, LiveCategory, LiveCategoryGroup } from '../api/live'

const store = useLiveStore()
const router = useRouter()
const searchFocused = ref(false)

/** 分类页每组默认展示的图标数；超出后折叠成「显示全部」。 */
const CATEGORY_PREVIEW = 12
const expandedGroups = ref<Record<string, boolean>>({})

function visibleAreas(group: LiveCategoryGroup): LiveCategory[] {
  return expandedGroups.value[group.id] ? group.children : group.children.slice(0, CATEGORY_PREVIEW)
}

function toggleGroup(groupId: string) {
  expandedGroups.value = { ...expandedGroups.value, [groupId]: !expandedGroups.value[groupId] }
}

const railItems: Array<{ key: LiveView; label: string; paths: string[] }> = [
  { key: 'home', label: '首页', paths: ['M3 10.5 12 3l9 7.5', 'M5 9.5V21h14V9.5'] },
  {
    key: 'follow',
    label: '关注',
    paths: ['M12 20s-7-4.4-7-9.2A4 4 0 0 1 12 8a4 4 0 0 1 7 2.8C19 15.6 12 20 12 20z'],
  },
  {
    key: 'category',
    label: '分类',
    paths: ['M4 4h7v7H4z', 'M13 4h7v7h-7z', 'M4 13h7v7H4z', 'M13 13h7v7h-7z'],
  },
  { key: 'history', label: '历史', paths: ['M12 7v5l3 2', 'M12 3a9 9 0 1 0 9 9'] },
]

const activeTitle = computed(
  () =>
    ({
      home: '直播',
      follow: '我的关注',
      category: '全部分类',
      history: '观看历史',
    })[store.view],
)

function platformName(id: string) {
  return store.platforms.find((item) => item.id === id)?.name || id
}

/** 单个关注房间的实时状态: true=直播中, false=未开播, undefined=状态还没查到。 */
function followLiveState(item: LiveFollow): boolean | undefined {
  return store.followStatus[`${item.platform}:${item.roomId}`]
}

/** 直播中的房间排在最前; 状态未知不影响原有先后顺序。 */
const sortedFollows = computed(() =>
  [...store.follows].sort((a, b) => {
    const aLive = followLiveState(a) === true ? 0 : 1
    const bLive = followLiveState(b) === true ? 0 : 1
    return aLive - bLive
  }),
)

/** 首页顶部只显示「当前选中的分区」，避免和左侧「分类」页重复。 */
const currentArea = computed(() => {
  const group = store.activeGroup
  if (!group) return ''
  const area = group.children.find((item) => item.id === store.activeAreaId)
  if (!area) return ''
  return `${group.name} · ${area.name}`
})

function clearArea() {
  // 回到「该一级分区的全部房间」；B 站支持 0=全部，其它平台退回第一个子分区。
  const group = store.activeGroup
  store.activeAreaId = group?.children[0]?.id && store.platform !== 'bilibili'
    ? group.children[0].id
    : '0'
  store.view = 'home'
  void store.loadRooms(true)
}

function reload() {
  store.clearSearch()
  if (store.view === 'follow') {
    void store.loadFollows()
    return
  }
  if (store.view === 'history') {
    void store.loadHistory()
    return
  }
  void store.loadCategories()
}

function openArea(groupId: string, areaId: string) {
  store.activeGroupId = groupId
  store.activeAreaId = areaId
  store.view = 'home'
  void store.loadRooms(true)
}

function openRoom(room: LiveRoomSummary) {
  void router.push({
    name: 'live-room',
    query: { roomId: room.roomId, platform: room.platform },
  })
}

function openFollow(item: LiveFollow) {
  void router.push({ name: 'live-room', query: { roomId: item.roomId, platform: item.platform } })
}

function openHistory(item: LiveRoomStats) {
  void router.push({ name: 'live-room', query: { roomId: item.roomId, platform: item.platform } })
}

onMounted(() => {
  void store.loadPlatforms()
  if (!store.categories.length) void store.loadCategories()
})
</script>

<style scoped>
.live-view {
  height: 100%;
  min-height: 0;
  overflow: hidden;
  display: flex;
  /* 与书架/音乐页同一套布局：整页居中限宽，rail 在限宽区域内。 */
  max-width: var(--content-max-width);
  margin: 0 auto;
  padding: 0 var(--space-6) var(--space-4);
}

.live-rail {
  flex-shrink: 0;
  width: 76px;
  padding: var(--space-4) 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  border-right: 1px solid var(--color-divider);
  overflow-y: auto;
  scrollbar-width: none;
}

.live-rail::-webkit-scrollbar {
  display: none;
}

.rail-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 5px;
  padding: 10px 4px;
  border-radius: var(--radius-lg);
  color: var(--color-text-tertiary);
  font-size: 12px;
  transition: all var(--duration-fast) var(--ease-out);
}

.rail-item svg {
  width: 22px;
  height: 22px;
}

.rail-item:hover {
  background: var(--color-bg-elevated);
  color: var(--color-text);
}

.rail-item.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.live-content {
  flex: 1;
  min-width: 0;
  padding: 0 0 0 var(--space-5);
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: auto;
}

.live-header {
  padding: var(--space-6) 0 var(--space-3);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  flex-shrink: 0;
}

.live-title {
  font-size: var(--text-2xl);
  font-weight: 700;
}

.live-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.live-search {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 12px;
  min-width: 220px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  transition: border-color var(--duration-fast) var(--ease-out);
}

.live-search.focused {
  border-color: var(--color-primary-border);
}

.live-search svg {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  color: var(--color-text-tertiary);
}

.live-search input {
  flex: 1;
  min-width: 0;
  border: none;
  background: transparent;
  color: var(--color-text);
  font-size: 13px;
  outline: none;
}

.live-search input::placeholder {
  color: var(--color-text-tertiary);
}

.search-clear {
  color: var(--color-text-tertiary);
  font-size: 16px;
  line-height: 1;
}

.search-clear:hover {
  color: var(--color-text);
}

.live-platforms {
  display: flex;
  gap: 6px;
  padding-bottom: 12px;
  margin-bottom: 12px;
  border-bottom: 1px solid var(--color-divider);
  flex-shrink: 0;
  overflow-x: auto;
  scrollbar-width: none;
}

.live-platforms::-webkit-scrollbar {
  display: none;
}

.platform-tab {
  flex-shrink: 0;
  padding: 8px 16px;
  border-radius: var(--radius-full);
  color: var(--color-text-secondary);
  font-size: 14px;
  font-weight: 500;
  transition: all var(--duration-fast) var(--ease-out);
}

.platform-tab:hover {
  color: var(--color-text);
}

.platform-tab.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.refresh-btn {
  padding: 8px 14px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  transition: all var(--duration-fast) var(--ease-out);
}

.refresh-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.refresh-btn:disabled {
  opacity: 0.6;
  cursor: default;
}

.live-filter {
  display: flex;
  gap: 8px;
  padding-bottom: var(--space-3);
  flex-shrink: 0;
}

.filter-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 8px 5px 12px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-size: 12px;
  font-weight: 600;
}

.filter-clear {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: color-mix(in srgb, var(--color-primary) 18%, transparent);
  color: var(--color-primary);
  font-size: 13px;
  line-height: 1;
}

.filter-clear:hover {
  background: color-mix(in srgb, var(--color-primary) 32%, transparent);
}

.area-chip {
  flex-shrink: 0;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
  padding: 6px 12px;
  font-size: 12px;
  transition: all var(--duration-fast) var(--ease-out);
}

.area-chip:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.area-chip.active {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
}

.category-groups {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.category-group h2 {
  margin: 0 0 10px;
  font-size: 15px;
  font-weight: 650;
  color: var(--color-text);
}

.category-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.category-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(76px, 1fr));
  gap: 14px 10px;
}

.category-tile {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 7px;
  padding: 2px;
  min-width: 0;
}

.category-icon {
  width: 52px;
  height: 52px;
  border-radius: var(--radius-lg);
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-bg-sunken);
  border: 1px solid var(--color-border-light);
  transition: transform var(--duration-fast) var(--ease-out), border-color var(--duration-fast);
}

.category-icon img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.category-icon-fallback {
  color: var(--color-text-tertiary);
  font-size: 18px;
  font-weight: 600;
}

.category-tile:hover .category-icon {
  transform: translateY(-2px);
  border-color: var(--color-primary-border);
}

.category-name {
  width: 100%;
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: 12px;
  line-height: 1.3;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.category-tile:hover .category-name {
  color: var(--color-primary);
}

.show-all {
  margin-top: 12px;
  padding: 5px 12px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.show-all:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.follow-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.follow-row {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 10px 12px;
  border-radius: var(--radius-lg);
  border: 1px solid transparent;
  text-align: left;
  transition: all var(--duration-fast) var(--ease-out);
}

.follow-row:hover {
  border-color: var(--color-border-light);
  background: var(--color-bg-elevated);
}

.follow-avatar {
  width: 38px;
  height: 38px;
  border-radius: 50%;
  object-fit: cover;
  flex-shrink: 0;
}

.follow-avatar.placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--color-bg-sunken);
  color: var(--color-text-tertiary);
  font-size: 15px;
}

.follow-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.follow-text strong {
  overflow: hidden;
  color: var(--color-text);
  font-size: 14px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.follow-text small {
  overflow: hidden;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.follow-platform {
  flex-shrink: 0;
  padding: 3px 9px;
  border-radius: var(--radius-md);
  background: var(--color-bg-sunken);
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.follow-status {
  flex-shrink: 0;
  padding: 3px 9px;
  border-radius: var(--radius-md);
  background: var(--color-bg-sunken);
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.follow-status.live {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.live-state {
  padding: 64px 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  color: var(--color-text-tertiary);
  font-size: 13px;
  text-align: center;
}

.state-btn {
  padding: 8px 18px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
}

.state-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.live-more {
  display: flex;
  justify-content: center;
  padding: var(--space-5) 0 var(--space-3);
}

@media (max-width: 720px) {
  .live-view {
    padding: 0 var(--space-3) var(--space-3);
    flex-direction: column-reverse;
  }

  .live-rail {
    width: 100%;
    flex-direction: row;
    justify-content: space-around;
    border-right: none;
    border-top: 1px solid var(--color-divider);
    padding: 6px 8px;
  }

  .rail-item {
    flex: 1;
    padding: 6px 4px;
  }

  .live-content {
    padding: 0 0 var(--space-3);
  }

  .live-header {
    flex-direction: column;
    align-items: stretch;
    gap: 10px;
    padding: var(--space-5) 0 var(--space-3);
  }

  .live-search {
    min-width: 0;
    flex: 1;
  }
}
</style>
