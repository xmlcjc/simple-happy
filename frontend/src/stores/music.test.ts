import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useMusicStore } from './music'
import { getMusicPlugins, searchMusic } from '../api/music'

vi.mock('../api/music', () => ({
  getMusicPlugins: vi.fn(),
  installMusicPlugin: vi.fn(),
  uninstallMusicPlugin: vi.fn(),
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
    createdAt: 0,
    updatedAt: 0,
  }
}

describe('music store', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
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
})
