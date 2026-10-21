import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useMusicLibraryStore } from './musicLibrary'
import {
  addMusicToPlaylist,
  getMusicFavorites,
  getMusicPlaylistItems,
  getMusicPlaylists,
  toggleMusicFavorite,
} from '../api/music'
import type { MusicItem } from '../api/music'

vi.mock('../api/music', () => ({
  getMusicFavorites: vi.fn(),
  toggleMusicFavorite: vi.fn(),
  getMusicPlaylists: vi.fn(),
  createMusicPlaylist: vi.fn(),
  deleteMusicPlaylist: vi.fn(),
  getMusicPlaylistItems: vi.fn(),
  addMusicToPlaylist: vi.fn(),
  removeMusicFromPlaylist: vi.fn(),
  reorderMusicPlaylist: vi.fn(),
  getMusicHistory: vi.fn(),
  clearMusicHistory: vi.fn(),
  getMusicStatsOverview: vi.fn(),
  recordMusicHistory: vi.fn(),
}))

const song = (id: string): MusicItem => ({
  platform: 'demo',
  id,
  title: `歌曲${id}`,
  artist: '歌手',
})

describe('music library store', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    setActivePinia(createPinia())
  })

  it('adds and removes favorites in place', async () => {
    vi.mocked(getMusicFavorites).mockResolvedValue([song('1')])
    vi.mocked(toggleMusicFavorite)
      .mockResolvedValueOnce({ favorited: false })
      .mockResolvedValueOnce({ favorited: true })
    const store = useMusicLibraryStore()

    await store.loadFavorites()
    expect(store.isFavorite(song('1'))).toBe(true)

    await store.toggleFavorite(song('1'))
    expect(store.favorites).toHaveLength(0)

    await store.toggleFavorite(song('2'))
    expect(store.favorites.map((item) => item.id)).toEqual(['2'])
  })

  it('refreshes playlist counts after adding songs', async () => {
    vi.mocked(getMusicPlaylists).mockResolvedValue([
      { id: 'p1', name: '默认', itemCount: 0, createdAt: 0, updatedAt: 0 },
    ])
    vi.mocked(addMusicToPlaylist).mockResolvedValue({ added: 2 })
    vi.mocked(getMusicPlaylistItems).mockResolvedValue([song('1'), song('2')])
    const store = useMusicLibraryStore()

    await store.loadPlaylists()
    const added = await store.addToPlaylist('p1', [song('1'), song('2')])

    expect(added).toBe(2)
    expect(addMusicToPlaylist).toHaveBeenCalledWith('p1', [song('1'), song('2')])
    expect(store.playlists[0].itemCount).toBe(0)
  })

  it('loads playlist items into the active playlist', async () => {
    vi.mocked(getMusicPlaylistItems).mockResolvedValue([song('9')])
    const store = useMusicLibraryStore()

    await store.loadPlaylistItems('p2')

    expect(store.activePlaylistId).toBe('p2')
    expect(store.playlistItems.map((item) => item.id)).toEqual(['9'])
  })
})
