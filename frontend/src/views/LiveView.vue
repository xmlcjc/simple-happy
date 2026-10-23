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
              placeholder="搜索直播间 / 主播 / 粘贴房间链接"
              @focus="searchFocused = true"
              @blur="searchFocused = false"
              @keyup.enter="submitSearch"
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
        <div class="history-stats">
          <div class="stat-card">
            <strong>{{ formatLiveDuration(liveStats.todaySeconds) }}</strong>
            <small>今日观看</small>
          </div>
          <div class="stat-card">
            <strong>{{ formatLiveDuration(liveStats.monthSeconds) }}</strong>
            <small>近 30 天</small>
          </div>
          <div class="stat-card">
            <strong>{{ formatLiveDuration(liveStats.totalSeconds) }}</strong>
            <small>累计观看</small>
          </div>
          <div class="stat-card">
            <strong>{{ liveStats.totalSessions }}</strong>
            <small>观看场次</small>
          </div>
        </div>
        <div v-if="!store.history.length" class="live-state">
          <p>还没有观看记录</p>
          <small>进入过的直播间会出现在这里</small>
        </div>
        <template v-else>
          <div class="history-toolbar">
            <span>最近看过 {{ store.history.length }} 个直播间</span>
            <button class="history-clear" @click="confirmClearHistory">清空记录</button>
          </div>
          <div class="follow-list">
            <div
              v-for="item in store.history"
              :key="`${item.platform}:${item.roomId}`"
              class="follow-row history-row"
            >
              <button class="history-main" @click="openHistory(item)">
                <img
                  v-if="item.cover || item.avatar"
                  class="history-cover"
                  :src="item.cover || item.avatar"
                  :alt="item.roomName"
                  referrerpolicy="no-referrer"
                />
                <span v-else class="history-cover placeholder">
                  {{ (item.anchorName || item.roomName || '?').slice(0, 1) }}
                </span>
                <span class="follow-text">
                  <strong>{{ item.roomName || item.anchorName || `房间 ${item.roomId}` }}</strong>
                  <small>{{ item.anchorName }} · 最近 {{ formatVisitedAt(item.visitedAt) }}</small>
                </span>
                <span class="follow-platform">{{ platformName(item.platform) }}</span>
              </button>
              <button class="history-remove" title="删除这条记录" @click="removeHistory(item)">×</button>
            </div>
          </div>
        </template>
      </template>

      <template v-else-if="store.view === 'category'">
        <div v-if="store.loading" class="live-state"><p>正在加载分区…</p></div>
        <div v-else class="category-groups">
          <section v-for="section in categorySections" :key="section.id" class="category-group">
            <h2>
              {{ section.name }}
              <span class="category-count">{{ section.tiles.length }}</span>
            </h2>
            <div class="category-grid">
              <button
                v-for="tile in visibleAreas(section)"
                :key="tile.id"
                class="category-tile"
                :title="tile.name"
                @click="openArea(tile.groupId, tile.id)"
              >
                <span class="category-icon" :style="tile.icon ? undefined : fallbackTint(tile.name)">
                  <img
                    v-if="tile.icon"
                    :src="tile.icon"
                    :alt="tile.name"
                    loading="lazy"
                    referrerpolicy="no-referrer"
                  />
                  <span v-else class="category-icon-fallback">{{ tile.name.slice(0, 1) }}</span>
                </span>
                <span class="category-name">{{ tile.name }}</span>
              </button>
            </div>
            <button
              v-if="section.tiles.length > CATEGORY_PREVIEW"
              class="show-all"
              @click="toggleGroup(section.id)"
            >
              {{ expandedGroups[section.id] ? '收起' : '显示全部' }}
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
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useLiveStore, type LiveView } from '../stores/live'
import { useLiveSettingsStore } from '../stores/liveSettings'
import LiveRoomGrid from '../components/live/LiveRoomGrid.vue'
import { parseLiveLink } from '../utils/liveLink'
import type { LiveFollow } from '../api/liveFollow'
import type { LiveHistoryEntry } from '../api/liveHistory'
import type { LiveRoomSummary, LiveCategory } from '../api/live'
import { getLiveStatsDaily, getLiveStatsSummary } from '../api/liveStats'

const store = useLiveStore()
const liveSettings = useLiveSettingsStore()
const router = useRouter()
const searchFocused = ref(false)

/** 观看历史页顶部的统计卡片（累计 / 今日 / 近 30 天）。 */
const liveStats = ref({
  todaySeconds: 0,
  monthSeconds: 0,
  totalSeconds: 0,
  totalSessions: 0,
})

function dateStr(offsetDays: number) {
  const date = new Date()
  date.setDate(date.getDate() - offsetDays)
  return date.toISOString().slice(0, 10)
}

function formatLiveDuration(seconds: number) {
  if (seconds <= 0) return '0分钟'
  if (seconds < 60) return '少于1分钟'
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  return hours > 0 ? `${hours}小时${rest}分` : `${Math.max(1, minutes)}分钟`
}

async function loadLiveStats() {
  const end = dateStr(0)
  const start = dateStr(29)
  try {
    const [daily, summary] = await Promise.all([
      getLiveStatsDaily(start, end),
      getLiveStatsSummary(),
    ])
    liveStats.value = {
      todaySeconds: daily.find((item) => item.date === end)?.seconds ?? 0,
      monthSeconds: daily.reduce((total, item) => total + item.seconds, 0),
      totalSeconds: summary.totalSeconds,
      totalSessions: summary.totalSessions,
    }
  } catch {
    // 统计读取失败不影响历史列表展示。
  }
}

watch(
  () => store.view,
  (view) => {
    if (view === 'history') void loadLiveStats()
  },
  { immediate: true },
)

/** 分类页每组默认展示的图标数；超出后折叠成「显示全部」。 */
const CATEGORY_PREVIEW = 12
const expandedGroups = ref<Record<string, boolean>>({})

interface CategorySection {
  id: string
  name: string
  /** 每个瓦片带上原始一级分区 id: 合并展示后仍要按正确 parent 拉房间。 */
  tiles: Array<LiveCategory & { groupId: string }>
}

/**
 * 分类页展示模型。
 *
 * 抖音这类平台的接口只给"一级分区"本身(没有二级子分类), 如果按原样分组会
 * 出现一排只有 1 个瓦片的空卡片。这里把这类稀疏分组合并成「其他分类」,
 * 游戏等真正有子分类的分组仍独立成卡片, 观感与斗鱼接近。
 */
const categorySections = computed<CategorySection[]>(() => {
  const rich: CategorySection[] = []
  const thin: CategorySection[] = []
  for (const group of store.categories) {
    const section: CategorySection = {
      id: group.id,
      name: group.name,
      tiles: group.children.map((area) => ({ ...area, groupId: group.id })),
    }
    if (group.children.length > 1) rich.push(section)
    else thin.push(section)
  }
  if (thin.length > 1) {
    return [
      ...rich,
      { id: '__other__', name: '其他分类', tiles: thin.flatMap((section) => section.tiles) },
    ]
  }
  return [...rich, ...thin]
})

function visibleAreas(section: CategorySection): CategorySection['tiles'] {
  return expandedGroups.value[section.id]
    ? section.tiles
    : section.tiles.slice(0, CATEGORY_PREVIEW)
}

/** 没有平台图标的分类(抖音等): 用名称哈希出稳定的主题色渐变, 比灰块更像"分类图标"。 */
function fallbackTint(name: string) {
  let hash = 0
  for (const char of name) hash = (hash * 31 + (char.codePointAt(0) ?? 0)) % 360
  return {
    '--cat-hue': String(hash),
  } as Record<string, string>
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

/** 首页顶部只显示「用户主动选中的分区」，避免和左侧「分类」页重复。 */
const currentArea = computed(() => {
  if (!store.areaFilterActive) return ''
  const group = store.activeGroup
  if (!group) return ''
  const area = group.children.find((item) => item.id === store.activeAreaId)
  if (!area) return ''
  return `${group.name} · ${area.name}`
})

function clearArea() {
  // 回到该一级分区的整组列表(斗鱼走 cate1 目录, 抖音回推荐, 虎牙回首个游戏)。
  store.clearAreaFilter()
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
  store.areaFilterActive = true
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

function openHistory(item: LiveHistoryEntry) {
  void router.push({ name: 'live-room', query: { roomId: item.roomId, platform: item.platform } })
}

/** 搜索框回车：先按直播间链接解析直接跳房，不是链接再走平台搜索。 */
function submitSearch() {
  const link = parseLiveLink(store.keyword)
  if (link) {
    store.clearSearch()
    void router.push({
      name: 'live-room',
      query: { roomId: link.roomId, platform: link.platform },
    })
    return
  }
  void store.runSearch()
}

function removeHistory(item: LiveHistoryEntry) {
  void store.removeHistoryRoom(item.platform, item.roomId)
}

function confirmClearHistory() {
  if (window.confirm('确定清空全部观看记录吗？')) void store.clearAllHistory()
}

/** 历史行的「最近」时间：今天显示时分，更早显示月-日。 */
function formatVisitedAt(visitedAt: number) {
  if (!visitedAt) return ''
  const date = new Date(visitedAt)
  const now = new Date()
  const pad = (value: number) => String(value).padStart(2, '0')
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate()
  if (sameDay) return `${pad(date.getHours())}:${pad(date.getMinutes())}`
  return `${date.getMonth() + 1}-${pad(date.getDate())}`
}

onMounted(() => {
  void store.loadPlatforms()
  if (!store.categories.length) void store.loadCategories()
  // 关注列表自动刷新: 离开直播页即停止, 间隔改动即时生效。
  store.startFollowAutoRefresh(liveSettings.followRefreshInterval)
})

watch(
  () => liveSettings.followRefreshInterval,
  (interval) => store.startFollowAutoRefresh(interval),
)

onBeforeUnmount(() => {
  store.stopFollowAutoRefresh()
})
</script>

<style scoped>
.live-view {
  height: 100%;
  min-height: 0;
  overflow: hidden;
  display: flex;
  /* 全宽铺满: 窄 rail 贴左缘, 内容列吃满剩余宽度。 */
  padding: 0 var(--space-6) var(--space-4) 0;
}

.live-rail {
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

.live-rail::-webkit-scrollbar {
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
  padding: 0 0 0 var(--space-4);
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
  gap: var(--space-4);
}

/* 每个一级分区一张卡片, 与斗鱼分类页的"分组 + 图标墙"观感一致。 */
.category-group {
  padding: 16px 18px;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
}

.category-group h2 {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 0 14px;
  font-size: 15px;
  font-weight: 650;
  color: var(--color-text);
}

.category-count {
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--color-bg-sunken);
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-weight: 500;
}

.category-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.category-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(84px, 1fr));
  gap: 16px 12px;
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
  width: 60px;
  height: 60px;
  border-radius: 18px;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  /* 无图标分类用名称哈希的色相做渐变, 有图标时会被 img 完全盖住。 */
  background:
    linear-gradient(
      135deg,
      color-mix(in srgb, hsl(var(--cat-hue, 30) 62% 58%) 18%, var(--color-bg-elevated)),
      color-mix(in srgb, hsl(calc(var(--cat-hue, 30) + 28) 62% 58%) 30%, var(--color-bg-elevated))
    );
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
  color: color-mix(in srgb, hsl(var(--cat-hue, 30) 62% 38%) 70%, var(--color-text));
  font-size: 22px;
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

/* ── 观看历史行：方形封面 + 悬停删除 ── */
.history-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 10px;
  color: var(--color-text-tertiary);
  font-size: 12px;
}

/* 观看历史页顶部统计卡片，规格与音乐/阅读历史一致。 */
.history-stats {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  padding-bottom: var(--space-3);
}

.stat-card {
  min-width: 120px;
  padding: 10px 14px;
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.stat-card strong {
  color: var(--color-text);
  font-size: 17px;
  font-weight: 600;
}

.stat-card small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.history-clear {
  padding: 4px 12px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.history-clear:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.history-row {
  position: relative;
  padding: 0;
}

.history-main {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 10px 40px 10px 12px;
  border-radius: var(--radius-lg);
  text-align: left;
}

.history-cover {
  width: 52px;
  height: 32px;
  border-radius: var(--radius-md);
  object-fit: cover;
  flex-shrink: 0;
  background: var(--color-bg-sunken);
}

.history-cover.placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
  font-size: 14px;
}

.history-remove {
  position: absolute;
  top: 50%;
  right: 10px;
  transform: translateY(-50%);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  color: var(--color-text-tertiary);
  font-size: 15px;
  line-height: 1;
  opacity: 0;
  transition: opacity var(--duration-fast) var(--ease-out);
}

.history-row:hover .history-remove {
  opacity: 1;
}

.history-remove:hover {
  background: var(--color-bg-sunken);
  color: var(--color-text);
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
