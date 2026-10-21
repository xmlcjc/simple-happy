import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import {
  getMusicLyric,
  getMusicMediaSource,
  getMusicPlugins,
  installMusicPlugin,
  searchMusic,
  searchMusicCollections,
  setMusicPluginEnabled,
  uninstallMusicPlugin,
  updateAllMusicPlugins,
  updateMusicPlugin,
  type MusicCollectionItem,
  type MusicItem,
  type MusicLyric,
  type MusicMediaSource,
  type MusicPlugin,
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
    if (activePlatform.value === platform) return
    activePlatform.value = platform
    results.value = []
    collections.value = []
    searchError.value = ''
    hasMore.value = false
    page.value = 1
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
    if (keyword.value.trim() && activePlatform.value) void search()
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

  async function search(reset = true) {
    const text = keyword.value.trim()
    if (!text) {
      results.value = []
      collections.value = []
      hasMore.value = false
      return
    }
    if (!activePlatform.value) {
      searchError.value = '请先安装并启用一个音乐插件'
      return
    }
    rememberSearch(text)
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
    enabledPlugins,
    activePlugin,
    loadPlugins,
    installPlugin,
    uninstallPlugin,
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
