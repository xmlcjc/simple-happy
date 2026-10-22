import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import {
  addMusicPluginSubscription,
  getMusicLyric,
  getMusicMediaSource,
  getMusicPluginSubscriptions,
  getMusicPlugins,
  installMusicPlugin,
  installMusicPluginFromUrl,
  removeMusicPluginSubscription,
  reorderMusicPlugins,
  searchMusic,
  searchMusicCollections,
  setMusicPluginEnabled,
  uninstallAllMusicPlugins,
  uninstallMusicPlugin,
  updateAllMusicPluginSubscriptions,
  updateAllMusicPlugins,
  updateMusicPlugin,
  updateMusicPluginSubscription,
  type MusicCollectionItem,
  type MusicItem,
  type MusicLyric,
  type MusicMediaSource,
  type MusicPlugin,
  type MusicPluginInstallResult,
  type MusicPluginSubscription,
  type MusicSearchType,
} from '../api/music'

/**
 * 音乐模块状态。
 *
 * 边界约定：
 * - 这里只管理「插件 + 搜索」这类跨组件数据；
 * - 播放队列与 <audio> 不放在全局 store，由音乐页内的 useMusicPlayer 持有，
 *   保证离开页面即停、不会影响其它页面；
 * - 收藏/歌单/历史在 musicLibrary store，后续阶段接入。
 */

const SEARCH_HISTORY_KEY = 'music-search-history'
const SEARCH_HISTORY_LIMIT = 20

/** node 测试环境没有 localStorage，读写都要能安全降级。 */
function readSearchHistory(): string[] {
  try {
    if (typeof localStorage === 'undefined') return []
    const parsed = JSON.parse(localStorage.getItem(SEARCH_HISTORY_KEY) || '[]')
    if (!Array.isArray(parsed)) return []
    return parsed.filter((item): item is string => typeof item === 'string').slice(
      0,
      SEARCH_HISTORY_LIMIT,
    )
  } catch {
    return []
  }
}

export const useMusicStore = defineStore('music', () => {
  const plugins = ref<MusicPlugin[]>([])
  const pluginsLoading = ref(false)
  const activePlatform = ref('')

  const keyword = ref('')
  /** 搜索类型：单曲 / 专辑 / 歌手。 */
  const searchType = ref<MusicSearchType>('music')
  const results = ref<MusicItem[]>([])
  /** 集合型搜索结果（专辑/歌手）。 */
  const collections = ref<MusicCollectionItem[]>([])
  const searching = ref(false)
  const searchError = ref('')
  const page = ref(1)
  const hasMore = ref(false)
  const searchHistory = ref<string[]>(readSearchHistory())

  /** 搜索范围：'all' 聚合全部已启用音源，'platform' 只搜当前音源。 */
  const searchScope = ref<'all' | 'platform'>('all')
  /** 聚合搜索的进度与失败来源（平台 -> 错误信息）。 */
  const searchProgress = ref({ done: 0, total: 0 })
  const platformErrors = ref<Record<string, string>>({})
  /** 聚合搜索时每个音源的翻页状态。 */
  const platformPages = ref<Record<string, number>>({})
  const platformHasMore = ref<Record<string, boolean>>({})

  const enabledPlugins = computed(() => plugins.value.filter((item) => item.enabled))
  const activePlugin = computed(
    () => plugins.value.find((item) => item.platform === activePlatform.value) ?? null,
  )

  async function loadPlugins() {
    pluginsLoading.value = true
    try {
      plugins.value = await getMusicPlugins()
      if (!activePlatform.value || !plugins.value.some((p) => p.platform === activePlatform.value)) {
        activePlatform.value = enabledPlugins.value[0]?.platform ?? ''
      }
    } catch (e) {
      searchError.value = (e as Error).message || '加载音乐插件失败'
    } finally {
      pluginsLoading.value = false
    }
  }

  async function installPlugin(source: string, fileName?: string) {
    const plugin = await installMusicPlugin(source, fileName)
    plugins.value = [
      ...plugins.value.filter((item) => item.platform !== plugin.platform),
      plugin,
    ]
    activePlatform.value = plugin.platform
    return plugin
  }

  async function uninstallPlugin(platform: string) {
    await uninstallMusicPlugin(platform)
    plugins.value = plugins.value.filter((item) => item.platform !== platform)
    if (activePlatform.value === platform) {
      activePlatform.value = enabledPlugins.value[0]?.platform ?? ''
    }
  }

  /** 按 URL 安装；支持 `.json` 订阅清单，返回逐条结果。 */
  async function installPluginFromUrl(url: string): Promise<MusicPluginInstallResult[]> {
    const results = await installMusicPluginFromUrl(url)
    if (results.some((item) => item.success)) {
      await loadPlugins()
    }
    return results
  }

  async function uninstallAllPlugins() {
    const result = await uninstallAllMusicPlugins()
    await loadPlugins()
    return result
  }

  /** 把插件移到指定下标，落库后按服务端顺序回读。 */
  async function movePlugin(platform: string, offset: number) {
    const order = plugins.value.map((item) => item.platform)
    const from = order.indexOf(platform)
    const to = from + offset
    if (from < 0 || to < 0 || to >= order.length) return
    order.splice(from, 1)
    order.splice(to, 0, platform)
    await reorderMusicPlugins(order)
    await loadPlugins()
  }

  const subscriptions = ref<MusicPluginSubscription[]>([])
  const subscriptionsLoading = ref(false)

  async function loadSubscriptions() {
    subscriptionsLoading.value = true
    try {
      subscriptions.value = await getMusicPluginSubscriptions()
    } finally {
      subscriptionsLoading.value = false
    }
  }

  async function addSubscription(name: string, url: string) {
    const item = await addMusicPluginSubscription(name, url)
    subscriptions.value = [...subscriptions.value, item]
    return item
  }

  async function removeSubscription(id: number) {
    await removeMusicPluginSubscription(id)
    subscriptions.value = subscriptions.value.filter((item) => item.id !== id)
  }

  async function updateSubscription(id: number) {
    const results = await updateMusicPluginSubscription(id)
    if (results.some((item) => item.success)) {
      await loadPlugins()
    }
    return results
  }

  async function updateAllSubscriptions() {
    const results = await updateAllMusicPluginSubscriptions()
    if (results.some((item) => item.success)) {
      await loadPlugins()
    }
    return results
  }

  /** 更新单个插件，并同步本地列表里的版本信息。 */
  async function updatePlugin(platform: string) {
    const plugin = await updateMusicPlugin(platform)
    plugins.value = plugins.value.map((item) =>
      item.platform === platform ? plugin : item,
    )
    return plugin
  }

  async function updateAllPlugins() {
    const result = await updateAllMusicPlugins()
    await loadPlugins()
    return result
  }

  async function togglePlugin(platform: string, enabled: boolean) {
    await setMusicPluginEnabled(platform, enabled)
    const plugin = plugins.value.find((item) => item.platform === platform)
    if (plugin) plugin.enabled = enabled
    if (!enabled && activePlatform.value === platform) {
      activePlatform.value = enabledPlugins.value[0]?.platform ?? ''
    }
  }

  function setActivePlatform(platform: string) {
    // 选中具体音源时切换为单源模式；"全部"由 setSearchAll 切回。
    searchScope.value = 'platform'
    if (activePlatform.value === platform) return
    activePlatform.value = platform
    results.value = []
    collections.value = []
    searchError.value = ''
    hasMore.value = false
    page.value = 1
    if (keyword.value.trim()) void search()
  }

  /** 切回"全部音源"聚合搜索。 */
  function setSearchAll() {
    if (searchScope.value === 'all') return
    searchScope.value = 'all'
    results.value = []
    collections.value = []
    searchError.value = ''
    platformErrors.value = {}
    hasMore.value = false
    page.value = 1
    if (keyword.value.trim()) void search()
  }

  /** 切换搜索类型；已有搜索结果时立即用新类型重搜。 */
  function setSearchType(type: MusicSearchType) {
    if (searchType.value === type) return
    searchType.value = type
    results.value = []
    collections.value = []
    searchError.value = ''
    hasMore.value = false
    page.value = 1
    if (keyword.value.trim() && (searchScope.value === 'all' || activePlatform.value)) {
      void search()
    }
  }

  function persistSearchHistory(next: string[]) {
    searchHistory.value = next
    try {
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(SEARCH_HISTORY_KEY, JSON.stringify(next))
      }
    } catch {
      // 存储不可用时只保留内存态。
    }
  }

  function rememberSearch(text: string) {
    const keyword = text.trim()
    if (!keyword) return
    persistSearchHistory(
      [keyword, ...searchHistory.value.filter((item) => item !== keyword)].slice(
        0,
        SEARCH_HISTORY_LIMIT,
      ),
    )
  }

  function removeSearchHistory(text: string) {
    persistSearchHistory(searchHistory.value.filter((item) => item !== text))
  }

  function clearSearchHistory() {
    persistSearchHistory([])
  }

  /** 单源请求，失败自动重试一次；仍失败则记录到 platformErrors。 */
  async function fetchSource(
    platform: string,
    text: string,
    type: MusicSearchType,
    targetPage: number,
  ) {
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        if (type === 'music') {
          const result = await searchMusic({ platform, keyword: text, page: targetPage })
          return { items: result.items, collections: [], isEnd: result.isEnd }
        }
        const result = await searchMusicCollections({
          platform,
          keyword: text,
          page: targetPage,
          searchType: type,
        })
        return { items: [], collections: result.items, isEnd: result.isEnd }
      } catch (error) {
        if (attempt === 0) {
          await new Promise((resolve) => setTimeout(resolve, 350))
          continue
        }
        platformErrors.value = {
          ...platformErrors.value,
          [platform]: (error as Error).message || '搜索失败',
        }
        return null
      }
    }
    return null
  }

  /**
   * 聚合搜索：并发请求全部已启用音源（4 路并发），谁先返回谁先显示；
   * 单个音源失败不影响其它音源。
   */
  async function searchAllSources(reset: boolean) {
    const text = keyword.value.trim()
    const plugins = enabledPlugins.value.slice()
    const type = searchType.value

    if (reset) {
      results.value = []
      collections.value = []
      page.value = 1
      hasMore.value = false
      platformErrors.value = {}
      platformPages.value = {}
      platformHasMore.value = {}
    }

    const seen = new Set(
      (type === 'music' ? results.value : collections.value).map(
        (item) => `${item.platform}:${item.id}`,
      ),
    )
    searchProgress.value = { done: 0, total: plugins.length }
    searching.value = true
    searchError.value = ''

    let cursor = 0
    let done = 0
    let anyMore = false

    const runWorker = async () => {
      while (cursor < plugins.length) {
        const plugin = plugins[cursor]
        cursor += 1
        const targetPage = reset ? 1 : (platformPages.value[plugin.platform] ?? 0) + 1
        const data = await fetchSource(plugin.platform, text, type, targetPage)
        if (data) {
          platformPages.value = { ...platformPages.value, [plugin.platform]: targetPage }
          platformHasMore.value = {
            ...platformHasMore.value,
            [plugin.platform]: !data.isEnd,
          }
          if (!data.isEnd) anyMore = true
          if (type === 'music') {
            const fresh: MusicItem[] = []
            for (const item of data.items) {
              const key = `${item.platform}:${item.id}`
              if (seen.has(key)) continue
              seen.add(key)
              fresh.push(item)
            }
            if (fresh.length) results.value = [...results.value, ...fresh]
          } else {
            const fresh: MusicCollectionItem[] = []
            for (const item of data.collections) {
              const key = `${item.platform}:${item.id}`
              if (seen.has(key)) continue
              seen.add(key)
              fresh.push(item)
            }
            if (fresh.length) collections.value = [...collections.value, ...fresh]
          }
        }
        done += 1
        searchProgress.value = { done, total: plugins.length }
      }
    }

    await Promise.all(
      Array.from({ length: Math.min(4, plugins.length) }, () => runWorker()),
    )

    hasMore.value = anyMore
    if (!results.value.length && !collections.value.length) {
      const failed = Object.keys(platformErrors.value).length
      if (failed >= plugins.length) {
        searchError.value = '所有音源搜索失败，请检查插件状态或网络'
      }
    }
    searching.value = false
  }

  async function search(reset = true) {
    const text = keyword.value.trim()
    if (!text) {
      results.value = []
      collections.value = []
      hasMore.value = false
      return
    }
    if (!enabledPlugins.value.length) {
      searchError.value = '请先安装并启用一个音乐插件'
      return
    }
    rememberSearch(text)

    if (searchScope.value === 'all') {
      await searchAllSources(reset)
      return
    }

    if (!activePlatform.value) {
      activePlatform.value = enabledPlugins.value[0]?.platform ?? ''
    }
    if (reset) {
      page.value = 1
      results.value = []
      collections.value = []
    }
    searching.value = true
    searchError.value = ''
    try {
      const type = searchType.value
      let isEnd = true
      let count = 0
      if (type === 'music') {
        const result = await searchMusic({
          platform: activePlatform.value,
          keyword: text,
          page: page.value,
        })
        results.value = reset ? result.items : [...results.value, ...result.items]
        isEnd = result.isEnd
        count = result.items.length
      } else {
        const result = await searchMusicCollections({
          platform: activePlatform.value,
          keyword: text,
          page: page.value,
          searchType: type,
        })
        collections.value = reset ? result.items : [...collections.value, ...result.items]
        isEnd = result.isEnd
        count = result.items.length
      }
      hasMore.value = !isEnd && count > 0
      if (!isEnd) page.value += 1
    } catch (e) {
      searchError.value = (e as Error).message || '搜索失败'
    } finally {
      searching.value = false
    }
  }

  function loadMore() {
    if (searching.value || !hasMore.value) return
    void search(false)
  }

  function clearSearch() {
    keyword.value = ''
    results.value = []
    collections.value = []
    searchError.value = ''
    platformErrors.value = {}
    searchProgress.value = { done: 0, total: 0 }
    platformPages.value = {}
    platformHasMore.value = {}
    hasMore.value = false
    page.value = 1
  }

  function mediaSource(item: MusicItem, quality?: string): Promise<MusicMediaSource> {
    return getMusicMediaSource({ platform: item.platform, item, quality })
  }

  function lyric(item: MusicItem): Promise<MusicLyric> {
    return getMusicLyric({ platform: item.platform, item })
  }

  return {
    plugins,
    pluginsLoading,
    activePlatform,
    keyword,
    searchType,
    results,
    collections,
    searching,
    searchError,
    page,
    hasMore,
    searchHistory,
    searchScope,
    searchProgress,
    platformErrors,
    setSearchAll,
    enabledPlugins,
    activePlugin,
    loadPlugins,
    installPlugin,
    installPluginFromUrl,
    uninstallPlugin,
    uninstallAllPlugins,
    movePlugin,
    subscriptions,
    subscriptionsLoading,
    loadSubscriptions,
    addSubscription,
    removeSubscription,
    updateSubscription,
    updateAllSubscriptions,
    togglePlugin,
    updatePlugin,
    updateAllPlugins,
    setActivePlatform,
    setSearchType,
    search,
    loadMore,
    clearSearch,
    rememberSearch,
    removeSearchHistory,
    clearSearchHistory,
    mediaSource,
    lyric,
  }
})
