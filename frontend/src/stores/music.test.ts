import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useMusicStore } from './music'
import {
  addMusicPluginSubscription,
  getMusicPlugins,
  getMusicPluginSubscriptions,
  installMusicPluginFromUrl,
  removeMusicPluginSubscription,
  reorderMusicPlugins,
  searchMusic,
  uninstallAllMusicPlugins,
  updateAllMusicPluginSubscriptions,
} from '../api/music'

vi.mock('../api/music', () => ({
  getMusicPlugins: vi.fn(),
  installMusicPlugin: vi.fn(),
  installMusicPluginFromUrl: vi.fn(),
  uninstallMusicPlugin: vi.fn(),
  uninstallAllMusicPlugins: vi.fn(),
  reorderMusicPlugins: vi.fn(),
  getMusicPluginSubscriptions: vi.fn(),
  addMusicPluginSubscription: vi.fn(),
  removeMusicPluginSubscription: vi.fn(),
  updateMusicPluginSubscription: vi.fn(),
  updateAllMusicPluginSubscriptions: vi.fn(),
  setMusicPluginEnabled: vi.fn(),
  searchMusic: vi.fn(),
  searchMusicCollections: vi.fn(),
  getMusicMediaSource: vi.fn(),
  getMusicLyric: vi.fn(),
}))

function installLocalStorage(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial))
  vi.stubGlobal('localStorage', {
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => values.set(key, value)),
    removeItem: vi.fn((key: string) => values.delete(key)),
  })
  return values
}

function plugin(platform: string, enabled = true) {
  return {
    platform,
    name: platform,
    version: '1.0.0',
    author: '',
    srcUrl: '',
    filePath: '',
    enabled,
    description: '',
    createdAt: 0,
    updatedAt: 0,
  }
}

function installResult(url: string, success = true) {
  return {
    url,
    success,
    platform: success ? 'demo' : '',
    name: success ? 'demo' : '',
    version: success ? '1.0.0' : '',
    error: success ? '' : '下载失败',
  }
}

describe('music store', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    vi.clearAllMocks()
    vi.unstubAllGlobals()
    setActivePinia(createPinia())
  })

  it('selects the first enabled plugin after loading', async () => {
    vi.mocked(getMusicPlugins).mockResolvedValue([plugin('disabled', false), plugin('demo')])
    const store = useMusicStore()

    await store.loadPlugins()

    expect(store.enabledPlugins.map((item) => item.platform)).toEqual(['demo'])
    expect(store.activePlatform).toBe('demo')
  })

  it('refuses to search without an enabled plugin', async () => {
    vi.mocked(getMusicPlugins).mockResolvedValue([])
    const store = useMusicStore()
    await store.loadPlugins()

    store.keyword = 'test'
    await store.search()

    expect(store.searchError).toContain('插件')
    expect(searchMusic).not.toHaveBeenCalled()
  })

  it('appends results and advances the page until the plugin reports the end', async () => {
    vi.mocked(getMusicPlugins).mockResolvedValue([plugin('demo')])
    vi.mocked(searchMusic).mockResolvedValue({
      items: [{ platform: 'demo', id: '1', title: '第一首', artist: 'A' }],
      isEnd: false,
      platform: 'demo',
    })
    const store = useMusicStore()
    await store.loadPlugins()

    store.keyword = 'test'
    await store.search()
    await store.search(false)

    expect(store.results).toHaveLength(2)
    expect(store.hasMore).toBe(true)
    expect(vi.mocked(searchMusic).mock.calls.map((call) => call[0].page)).toEqual([1, 2])
  })

  it('records search history with dedupe, delete and clear', () => {
    const values = installLocalStorage()
    const store = useMusicStore()

    store.rememberSearch(' 晴天 ')
    store.rememberSearch('七里香')
    store.rememberSearch('晴天')

    expect(store.searchHistory).toEqual(['晴天', '七里香'])
    expect(JSON.parse(values.get('music-search-history') || '[]')).toEqual(['晴天', '七里香'])

    store.removeSearchHistory('晴天')
    expect(store.searchHistory).toEqual(['七里香'])

    store.clearSearchHistory()
    expect(store.searchHistory).toEqual([])
  })

  it('restores search history from localStorage', () => {
    installLocalStorage({ 'music-search-history': '["a","b","a"]' })
    const store = useMusicStore()
    expect(store.searchHistory).toEqual(['a', 'b', 'a'])
  })

  it('reloads plugins after a successful url install', async () => {
    vi.mocked(installMusicPluginFromUrl).mockResolvedValue([installResult('https://x/p.js')])
    vi.mocked(getMusicPlugins).mockResolvedValue([plugin('demo')])
    const store = useMusicStore()

    const results = await store.installPluginFromUrl('https://x/p.js')

    expect(results[0].success).toBe(true)
    expect(getMusicPlugins).toHaveBeenCalled()
    expect(store.plugins.map((item) => item.platform)).toEqual(['demo'])
  })

  it('skips reload when every url install failed', async () => {
    vi.mocked(installMusicPluginFromUrl).mockResolvedValue([
      installResult('https://x/missing.js', false),
    ])
    const store = useMusicStore()

    await store.installPluginFromUrl('https://x/missing.js')

    expect(getMusicPlugins).not.toHaveBeenCalled()
  })

  it('moves a plugin and persists the new order', async () => {
    vi.mocked(getMusicPlugins).mockResolvedValue([plugin('a'), plugin('b'), plugin('c')])
    vi.mocked(reorderMusicPlugins).mockResolvedValue({ reordered: 3 })
    const store = useMusicStore()
    await store.loadPlugins()

    await store.movePlugin('c', -2)

    expect(reorderMusicPlugins).toHaveBeenCalledWith(['c', 'a', 'b'])
  })

  it('ignores moves beyond the list boundaries', async () => {
    vi.mocked(getMusicPlugins).mockResolvedValue([plugin('a'), plugin('b')])
    const store = useMusicStore()
    await store.loadPlugins()

    await store.movePlugin('a', -1)
    await store.movePlugin('b', 1)

    expect(reorderMusicPlugins).not.toHaveBeenCalled()
  })

  it('manages subscriptions in local state', async () => {
    vi.mocked(getMusicPluginSubscriptions).mockResolvedValue([
      { id: 1, name: '清单', url: 'https://x/plugins.json', createdAt: 0 },
    ])
    vi.mocked(addMusicPluginSubscription).mockResolvedValue({
      id: 2,
      name: 'single.js',
      url: 'https://x/single.js',
      createdAt: 1,
    })
    vi.mocked(removeMusicPluginSubscription).mockResolvedValue({ removed: true })
    const store = useMusicStore()

    await store.loadSubscriptions()
    expect(store.subscriptions.map((item) => item.id)).toEqual([1])

    await store.addSubscription('', 'https://x/single.js')
    expect(store.subscriptions.map((item) => item.id)).toEqual([1, 2])

    await store.removeSubscription(1)
    expect(store.subscriptions.map((item) => item.id)).toEqual([2])
  })

  it('reloads plugins when a subscription update succeeded', async () => {
    vi.mocked(updateAllMusicPluginSubscriptions).mockResolvedValue([
      installResult('https://x/p.js'),
    ])
    vi.mocked(getMusicPlugins).mockResolvedValue([plugin('demo')])
    const store = useMusicStore()

    await store.updateAllSubscriptions()

    expect(getMusicPlugins).toHaveBeenCalledTimes(1)
  })

  it('uninstalls every plugin and refreshes the list', async () => {
    vi.mocked(uninstallAllMusicPlugins).mockResolvedValue({ removed: 2 })
    vi.mocked(getMusicPlugins).mockResolvedValue([])
    const store = useMusicStore()

    const result = await store.uninstallAllPlugins()

    expect(result.removed).toBe(2)
    expect(store.plugins).toEqual([])
  })
})
