import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import {
  addMusicToPlaylist,
  clearMusicHistory,
  createMusicPlaylist,
  deleteMusicPlaylist,
  getMusicFavorites,
  getMusicHistory,
  getMusicPlaylistItems,
  getMusicPlaylists,
  getMusicStatsOverview,
  getLocalMusic,
  pickLocalMusic,
  recordMusicHistory,
  removeLocalMusic,
  removeMusicFromPlaylist,
  reorderMusicPlaylist,
  toggleMusicFavorite,
  type LocalTrack,
  type MusicHistoryEntry,
  type MusicItem,
  type MusicPlaylistSummary,
  type MusicStatsOverview,
} from '../api/music'

/**
 * 音乐曲库状态：收藏、歌单、历史、统计。
 *
 * 与播放器分开：播放器是页面级 composable，这里只负责持久化数据，
 * 这样「播放」与「曲库」互不影响，出问题也容易定位。
 */
export const useMusicLibraryStore = defineStore('musicLibrary', () => {
  const favorites = ref<MusicItem[]>([])
  const playlists = ref<MusicPlaylistSummary[]>([])
  const activePlaylistId = ref('')
  const playlistItems = ref<MusicItem[]>([])
  const history = ref<MusicHistoryEntry[]>([])
  const stats = ref<MusicStatsOverview | null>(null)
  const localTracks = ref<LocalTrack[]>([])
  const loading = ref(false)

  const favoriteKeys = computed(
    () => new Set(favorites.value.map((item) => `${item.platform}:${item.id}`)),
  )

  function isFavorite(item: MusicItem) {
    return favoriteKeys.value.has(`${item.platform}:${item.id}`)
  }

  async function loadFavorites() {
    favorites.value = await getMusicFavorites()
  }

  async function toggleFavorite(item: MusicItem) {
    const result = await toggleMusicFavorite({ platform: item.platform, item })
    if (result.favorited) {
      favorites.value = [
        item,
        ...favorites.value.filter(
          (entry) => !(entry.platform === item.platform && entry.id === item.id),
        ),
      ]
    } else {
      favorites.value = favorites.value.filter(
        (entry) => !(entry.platform === item.platform && entry.id === item.id),
      )
    }
    return result.favorited
  }

  async function loadPlaylists() {
    playlists.value = await getMusicPlaylists()
  }

  async function createPlaylist(name: string) {
    const playlist = await createMusicPlaylist(name)
    playlists.value = [...playlists.value, playlist]
    return playlist
  }

  async function deletePlaylist(id: string) {
    await deleteMusicPlaylist(id)
    playlists.value = playlists.value.filter((item) => item.id !== id)
    if (activePlaylistId.value === id) {
      activePlaylistId.value = ''
      playlistItems.value = []
    }
  }

  async function loadPlaylistItems(id: string) {
    loading.value = true
    try {
      activePlaylistId.value = id
      playlistItems.value = await getMusicPlaylistItems(id)
    } finally {
      loading.value = false
    }
  }

  async function addToPlaylist(id: string, items: MusicItem[]) {
    if (!items.length) return 0
    const result = await addMusicToPlaylist(id, items)
    await loadPlaylists()
    if (activePlaylistId.value === id) await loadPlaylistItems(id)
    return result.added
  }

  async function removeFromPlaylist(id: string, item: MusicItem) {
    await removeMusicFromPlaylist(id, item.platform, item.id)
    if (activePlaylistId.value === id) {
      playlistItems.value = playlistItems.value.filter(
        (entry) => !(entry.platform === item.platform && entry.id === item.id),
      )
    }
    await loadPlaylists()
  }

  async function reorderPlaylist(id: string, items: MusicItem[]) {
    await reorderMusicPlaylist(
      id,
      items.map((item) => ({ platform: item.platform, itemId: item.id })),
    )
    playlistItems.value = items
  }

  async function loadHistory(limit = 100) {
    history.value = await getMusicHistory(limit)
  }

  async function clearHistory() {
    await clearMusicHistory()
    history.value = []
  }

  async function loadStats(days = 30) {
    stats.value = await getMusicStatsOverview(days)
  }

  /** 播放器定时上报；不主动刷新列表，避免每 30 秒打一次数据库。 */
  async function record(item: MusicItem, seconds: number, finished = false) {
    if (seconds <= 0 && !finished) return
    await recordMusicHistory({ platform: item.platform, item, seconds, finished })
  }

  async function loadLocal() {
    localTracks.value = await getLocalMusic()
  }

  /** 弹出系统文件选择框；返回本次新增的数量。 */
  async function pickLocal() {
    const imported = await pickLocalMusic()
    if (imported.length) await loadLocal()
    return imported.length
  }

  async function removeLocal(path: string) {
    await removeLocalMusic(path)
    localTracks.value = localTracks.value.filter((track) => track.path !== path)
  }

  return {
    favorites,
    playlists,
    activePlaylistId,
    playlistItems,
    history,
    stats,
    localTracks,
    loading,
    favoriteKeys,
    isFavorite,
    loadFavorites,
    toggleFavorite,
    loadPlaylists,
    createPlaylist,
    deletePlaylist,
    loadPlaylistItems,
    addToPlaylist,
    removeFromPlaylist,
    reorderPlaylist,
    loadHistory,
    clearHistory,
    loadStats,
    record,
    loadLocal,
    pickLocal,
    removeLocal,
  }
})
