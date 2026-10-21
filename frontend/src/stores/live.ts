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
  unfollowLiveRoom,
  type LiveFollow,
} from '../api/liveFollow'
import {
  clearLiveHistory,
  deleteLiveHistoryRoom,
  getLiveHistory,
  type LiveHistoryEntry,
} from '../api/liveHistory'

/** 直播首页的四个视图, 对应左侧栏。 */
export type LiveView = 'home' | 'follow' | 'category' | 'history'

/** 各平台房间列表的每页条数(由后端固定):
 *  B 站 page_size=30; 虎牙、斗鱼每页 120; 抖音 count=15。
 *  早先统一按 30 判断「还有下一页」, 抖音每页只有 15 条, 于是永远不显示「加载更多」。 */
const ROOM_PAGE_SIZES: Record<string, number> = {
  bilibili: 30,
  huya: 120,
  douyu: 120,
  douyin: 15,
}

export const useLiveStore = defineStore('live', () => {
  const platforms = ref<LivePlatform[]>([])
  const platform = ref('bilibili')
  const categories = ref<LiveCategoryGroup[]>([])
  const activeGroupId = ref<string>('')
  const activeAreaId = ref<string>('0')
  /** 首页是否显示当前分区筛选 chip。
   *  首屏默认落在「整组/推荐」不算用户筛选；只有从「分类」页主动选过二级分区才显示，
   *  否则斗鱼/抖音这类必须带具体分区的平台会一直挂着一个清不掉的 chip。 */
  const areaFilterActive = ref(false)
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
  const history = ref<LiveHistoryEntry[]>([])
  /** 关注列表的实时开播状态, key 为 `platform:roomId`; 查询失败的房间保持缺省(未知)。 */
  const followStatus = ref<Record<string, boolean>>({})
  /** 关注列表自动刷新定时器(进直播页启动, 离开时停止)。 */
  let followRefreshTimer: number | undefined

  const activeGroup = computed<LiveCategoryGroup | null>(() =>
    categories.value.find((g) => g.id === activeGroupId.value) ?? null,
  )

  const isAll = computed(() => activeAreaId.value === '0')
  /** 哪些平台支持「整个一级分区」的列表。
   *  虎牙的接口只认具体游戏(gid), 一级 id 是业务类型, 拿它查会得到空列表。 */
  const supportsAllArea = computed(() => platform.value !== 'huya')
  const followedKeys = computed(() => new Set(follows.value.map((item) => `${item.platform}:${item.roomId}`)))

  /** 该平台的「无筛选」落点: B 站/斗鱼传 0, 抖音传 0 由后端退回推荐(720,1),
   *  虎牙退回首个游戏(接口限制)。 */
  function defaultAreaId(group: LiveCategoryGroup | null): string {
    if (platform.value === 'huya') return group?.children[0]?.id ?? '0'
    return '0'
  }

  /** 默认一级分区: 抖音没有「全部」, 用空 parent 让后端落到推荐分区;
   *  其它平台沿用分区列表的第一个。 */
  function defaultGroupId(): string {
    if (platform.value === 'douyin') return ''
    return categories.value[0]?.id ?? ''
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
        activeGroupId.value = defaultGroupId()
        activeAreaId.value = defaultAreaId(activeGroup.value)
        areaFilterActive.value = false
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
    areaFilterActive.value = false
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
    areaFilterActive.value = false
    loadRooms(true)
  }

  function selectArea(areaId: string) {
    activeAreaId.value = areaId
    areaFilterActive.value = true
    loadRooms(true)
  }

  /** 清空首页分区筛选: 回到该一级分区的整组列表并隐藏 chip。 */
  function clearAreaFilter() {
    // 抖音的「无筛选」是推荐分区(没有一级分区), 其余平台回到当前一级分区整组;
    // 虎牙仍由 defaultAreaId 落到首个游戏。
    activeGroupId.value = platform.value === 'douyin'
      ? ''
      : (activeGroupId.value || categories.value[0]?.id || '')
    activeAreaId.value = defaultAreaId(activeGroup.value)
    areaFilterActive.value = false
    view.value = 'home'
    void loadRooms(true)
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
      hasMore.value = list.length >= (ROOM_PAGE_SIZES[platform.value] ?? 30)
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

  async function removeHistoryRoom(platform: string, roomId: string) {
    try {
      await deleteLiveHistoryRoom(platform, roomId)
      history.value = history.value.filter(
        (item) => !(item.platform === platform && item.roomId === roomId),
      )
    } catch (e) {
      error.value = (e as Error).message || '删除历史失败'
    }
  }

  async function clearAllHistory() {
    try {
      await clearLiveHistory()
      history.value = []
    } catch (e) {
      error.value = (e as Error).message || '清空历史失败'
    }
  }

  /** 定时刷新关注列表的开播状态; intervalSec <= 0 只停不启。 */
  function startFollowAutoRefresh(intervalSec: number) {
    if (followRefreshTimer !== undefined) {
      window.clearInterval(followRefreshTimer)
      followRefreshTimer = undefined
    }
    if (intervalSec <= 0) return
    followRefreshTimer = window.setInterval(
      () => {
        if (follows.value.length) void refreshFollowStatus()
      },
      intervalSec * 1000,
    )
  }

  function stopFollowAutoRefresh() {
    startFollowAutoRefresh(0)
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
    areaFilterActive,
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
    clearAreaFilter,
    loadRooms,
    loadMore,
    runSearch,
    clearSearch,
    loadFollows,
    refreshFollowStatus,
    loadHistory,
    removeHistoryRoom,
    clearAllHistory,
    startFollowAutoRefresh,
    stopFollowAutoRefresh,
    toggleFollow,
    fetchRoomDetail,
    fetchStreamUrls,
  }
})
