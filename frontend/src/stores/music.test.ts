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
  getMusicMediaSource: vi.fn(),
  getMusicLyric: vi.fn(),
}))

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
})
