import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import {
  getLiveCategories,
  getLivePlatforms,
  getLiveRoomDetail,
  getLiveRooms,
  getLiveStreamUrls,
  searchLiveRooms,
  type LiveCategoryGroup,
  type LivePlatform,
  type LiveRoomSummary,
  type LiveStreamInfo,
} from '../api/live'
import {
  followLiveRoom,
  getLiveFollows,
  getLiveHistory,
  unfollowLiveRoom,
  type LiveFollow,
} from '../api/liveFollow'
import type { LiveRoomStats } from '../api/liveStats'

/** 直播首页的四个视图, 对应左侧栏。 */
export type LiveView = 'home' | 'follow' | 'category' | 'history'

export const useLiveStore = defineStore('live', () => {
  const platforms = ref<LivePlatform[]>([])
  const platform = ref('bilibili')
  const categories = ref<LiveCategoryGroup[]>([])
  const activeGroupId = ref<string>('')
  const activeAreaId = ref<string>('0')
  const rooms = ref<LiveRoomSummary[]>([])
  const page = ref(1)
  const hasMore = ref(true)
  const loading = ref(false)
  const loadingMore = ref(false)
  const error = ref('')

  const view = ref<LiveView>('home')
  const keyword = ref('')
  const searchResults = ref<LiveRoomSummary[]>([])
  const searching = ref(false)
  const follows = ref<LiveFollow[]>([])
  const history = ref<LiveRoomStats[]>([])
  /** 关注列表的实时开播状态, key 为 `platform:roomId`; 查询失败的房间保持缺省(未知)。 */
  const followStatus = ref<Record<string, boolean>>({})

  const activeGroup = computed<LiveCategoryGroup | null>(() =>
    categories.value.find((g) => g.id === activeGroupId.value) ?? null,
  )

  const isAll = computed(() => activeAreaId.value === '0')
  /** 只有 B 站支持「该一级分区下全部二级分区」; 虎牙/斗鱼的接口必须落到具体游戏。 */
  const supportsAllArea = computed(() => platform.value === 'bilibili')
  const followedKeys = computed(() => new Set(follows.value.map((item) => `${item.platform}:${item.roomId}`)))

  function defaultAreaId(group: LiveCategoryGroup | null): string {
    if (!supportsAllArea.value) return group?.children[0]?.id ?? '0'
    return '0'
  }

  async function loadPlatforms() {
    try {
      platforms.value = await getLivePlatforms()
    } catch (e) {
      // 平台列表拉取失败不阻断, 默认仍可用 B 站。
    }
  }

  async function loadCategories() {
    loading.value = true
    error.value = ''
    try {
      categories.value = await getLiveCategories(platform.value)
      const first = categories.value[0] ?? null
      if (first) {
        activeGroupId.value = first.id
        activeAreaId.value = defaultAreaId(first)
        await loadRooms(true)
      }
    } catch (e) {
      error.value = (e as Error).message || '加载分区失败'
    } finally {
      loading.value = false
    }
  }

  async function switchPlatform(id: string) {
    if (platform.value === id) return
    platform.value = id
    categories.value = []
    activeGroupId.value = ''
    activeAreaId.value = '0'
    rooms.value = []
    searchResults.value = []
    await loadCategories()
    if (view.value === 'history') await loadHistory()
    if (view.value === 'follow') await loadFollows()
  }

  async function setView(next: LiveView) {
    view.value = next
    if (next === 'follow') await loadFollows()
    if (next === 'history') await loadHistory()
    if (next === 'category' && !categories.value.length) await loadCategories()
  }

  function selectGroup(groupId: string) {
    activeGroupId.value = groupId
    activeAreaId.value = defaultAreaId(activeGroup.value)
    loadRooms(true)
  }

  function selectArea(areaId: string) {
    activeAreaId.value = areaId
    loadRooms(true)
  }

  async function loadRooms(reset: boolean) {
    if (reset) {
      page.value = 1
      hasMore.value = true
      rooms.value = []
    }
    const currentPage = page.value
    if (!hasMore.value && !reset) return
    const isMore = currentPage > 1
    if (isMore) loadingMore.value = true
    else loading.value = true
    error.value = ''
    try {
      const list = await getLiveRooms({
        platform: platform.value,
        parentAreaId: activeGroupId.value,
        areaId: activeAreaId.value,
        page: currentPage,
      })
      if (reset) rooms.value = list
      else rooms.value.push(...list)
      hasMore.value = list.length >= 30
      if (!reset) page.value += 1
    } catch (e) {
      error.value = (e as Error).message || '加载房间失败'
    } finally {
      if (isMore) loadingMore.value = false
      else loading.value = false
    }
  }

  async function loadMore() {
    page.value += 1
    await loadRooms(false)
  }

  async function runSearch() {
    const text = keyword.value.trim()
    if (!text) {
      searchResults.value = []
      return
    }
    searching.value = true
    error.value = ''
    try {
      searchResults.value = await searchLiveRooms({ platform: platform.value, keyword: text })
    } catch (e) {
      error.value = (e as Error).message || '搜索失败'
      searchResults.value = []
    } finally {
      searching.value = false
    }
  }

  function clearSearch() {
    keyword.value = ''
    searchResults.value = []
  }

  async function loadFollows() {
    try {
      follows.value = await getLiveFollows()
      // 状态查询不阻塞列表渲染: 先出名单, 开播标识随后逐个补上。
      void refreshFollowStatus()
    } catch (e) {
      error.value = (e as Error).message || '加载关注失败'
    }
  }

  /** 跨平台逐个查询关注房间的开播状态; 单房间失败不影响其它房间。 */
  async function refreshFollowStatus() {
    const list = follows.value
    if (!list.length) {
      followStatus.value = {}
      return
    }
    const results = await Promise.allSettled(
      list.map((item) => getLiveRoomDetail(item.platform, item.roomId)),
    )
    // 刷新期间关注列表被重载过则放弃本次结果, 避免旧数据盖掉新名单对应的状态。
    if (follows.value !== list) return
    const next: Record<string, boolean> = {}
    results.forEach((result, index) => {
      if (result.status !== 'fulfilled') return
      const item = list[index]
      next[`${item.platform}:${item.roomId}`] = result.value.isLive
    })
    followStatus.value = next
  }

  async function loadHistory() {
    try {
      history.value = await getLiveHistory(50)
    } catch (e) {
      error.value = (e as Error).message || '加载历史失败'
    }
  }

  async function toggleFollow(room: {
    platform: string
    roomId: string
    roomName?: string
    anchorName?: string
    avatar?: string
  }) {
    const key = `${room.platform}:${room.roomId}`
    if (followedKeys.value.has(key)) {
      await unfollowLiveRoom(room.platform, room.roomId)
    } else {
      await followLiveRoom({
        platform: room.platform,
        roomId: room.roomId,
        roomName: room.roomName,
        anchorName: room.anchorName,
        avatar: room.avatar,
      })
    }
    await loadFollows()
  }

  async function fetchRoomDetail(roomId: string) {
    return getLiveRoomDetail(platform.value, roomId)
  }

  async function fetchStreamUrls(roomId: string): Promise<LiveStreamInfo> {
    return getLiveStreamUrls(platform.value, roomId)
  }

  return {
    platforms,
    platform,
    categories,
    activeGroupId,
    activeAreaId,
    rooms,
    page,
    hasMore,
    loading,
    loadingMore,
    error,
    view,
    keyword,
    searchResults,
    searching,
    follows,
    history,
    followStatus,
    activeGroup,
    isAll,
    supportsAllArea,
    followedKeys,
    loadPlatforms,
    loadCategories,
    switchPlatform,
    setView,
    selectGroup,
    selectArea,
    loadRooms,
    loadMore,
    runSearch,
    clearSearch,
    loadFollows,
    refreshFollowStatus,
    loadHistory,
    toggleFollow,
    fetchRoomDetail,
    fetchStreamUrls,
  }
})
