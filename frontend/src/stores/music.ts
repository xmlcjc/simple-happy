import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import {
  getMusicLyric,
  getMusicMediaSource,
  getMusicPlugins,
  installMusicPlugin,
  searchMusic,
  setMusicPluginEnabled,
  uninstallMusicPlugin,
  type MusicItem,
  type MusicLyric,
  type MusicMediaSource,
  type MusicPlugin,
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
export const useMusicStore = defineStore('music', () => {
  const plugins = ref<MusicPlugin[]>([])
  const pluginsLoading = ref(false)
  const activePlatform = ref('')

  const keyword = ref('')
  const results = ref<MusicItem[]>([])
  const searching = ref(false)
  const searchError = ref('')
  const page = ref(1)
  const hasMore = ref(false)

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
    searchError.value = ''
    hasMore.value = false
    page.value = 1
  }

  async function search(reset = true) {
    const text = keyword.value.trim()
    if (!text) {
      results.value = []
      hasMore.value = false
      return
    }
    if (!activePlatform.value) {
      searchError.value = '请先安装并启用一个音乐插件'
      return
    }
    if (reset) {
      page.value = 1
      results.value = []
    }
    searching.value = true
    searchError.value = ''
    try {
      const result = await searchMusic({
        platform: activePlatform.value,
        keyword: text,
        page: page.value,
      })
      results.value = reset ? result.items : [...results.value, ...result.items]
      hasMore.value = !result.isEnd && result.items.length > 0
      if (!result.isEnd) page.value += 1
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
    searchError.value = ''
    hasMore.value = false
    page.value = 1
  }

  function mediaSource(item: MusicItem): Promise<MusicMediaSource> {
    return getMusicMediaSource({ platform: item.platform, item })
  }

  function lyric(item: MusicItem): Promise<MusicLyric> {
    return getMusicLyric({ platform: item.platform, item })
  }

  return {
    plugins,
    pluginsLoading,
    activePlatform,
    keyword,
    results,
    searching,
    searchError,
    page,
    hasMore,
    enabledPlugins,
    activePlugin,
    loadPlugins,
    installPlugin,
    uninstallPlugin,
    togglePlugin,
    setActivePlatform,
    search,
    loadMore,
    clearSearch,
    mediaSource,
    lyric,
  }
})
