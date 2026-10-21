// 音乐模块 IPC 封装。与 Rust 侧 `service::music` 一一对应, 只做类型与调用,
// 不在这里写播放/队列逻辑。
import { invokeEnvelope } from './invoke'

/** 对齐 MusicFree 的 IMusicItem; 插件私有字段通过索引签名原样保留。 */
export interface MusicItem {
  platform: string
  id: string
  title: string
  artist: string
  album?: string
  artwork?: string
  duration?: number
  [key: string]: unknown
}

export interface MusicPlugin {
  platform: string
  name: string
  version: string
  author: string
  srcUrl: string
  filePath: string
  enabled: boolean
  createdAt: number
  updatedAt: number
}

export interface MusicSearchPage {
  items: MusicItem[]
  isEnd: boolean
  platform: string
}

/** 搜索类型：music 为单曲，其余为集合型条目。 */
export type MusicSearchType = 'music' | 'album' | 'artist' | 'sheet'

/** 专辑 / 歌手 / 歌单。歌手条目的 name/avatar 已在后端归一成 title/artwork。 */
export interface MusicCollectionItem {
  platform: string
  id: string
  title: string
  artist?: string
  artwork?: string
  description?: string
  worksNum?: number
  playCount?: number
  date?: string
  [key: string]: unknown
}

export interface MusicCollectionPage {
  items: MusicCollectionItem[]
  isEnd: boolean
  platform: string
  searchType: string
}

/** 详情页类型：专辑 / 歌手 / 歌单 / 榜单。 */
export type MusicCollectionKind = 'album' | 'artist' | 'sheet' | 'toplist'

/** 集合详情：专辑/歌单/榜单给 items，歌手的专辑作品给 collections。 */
export interface MusicCollectionDetail {
  item: MusicCollectionItem | null
  items: MusicItem[]
  collections: MusicCollectionItem[]
  isEnd: boolean
}

/** 榜单分组。 */
export interface MusicSheetGroup {
  title: string
  items: MusicCollectionItem[]
}

export interface MusicTag {
  id: string
  title: string
}

export interface MusicTagGroup {
  title: string
  tags: MusicTag[]
}

/** 推荐歌单的 tag 列表；pinned 是插件固定的入口歌单。 */
export interface MusicRecommendTags {
  pinned: MusicCollectionItem[]
  groups: MusicTagGroup[]
}

export interface MusicMediaSource {
  url: string
  quality: string
  headers: Record<string, unknown>
}

export interface MusicLyric {
  raw: string
  translation: string
}

export function getMusicPlugins(): Promise<MusicPlugin[]> {
  return invokeEnvelope('music_plugin_list', {})
}

export function installMusicPlugin(source: string, fileName?: string): Promise<MusicPlugin> {
  return invokeEnvelope('music_plugin_install', { req: { source, fileName } })
}

export function uninstallMusicPlugin(platform: string): Promise<{ removed: boolean }> {
  return invokeEnvelope('music_plugin_uninstall', { platform })
}

export function updateMusicPlugin(platform: string): Promise<MusicPlugin> {
  return invokeEnvelope('music_plugin_update', { platform })
}

export interface MusicPluginUpdateAllResult {
  updated: number
  failures: Array<{ platform: string; error: string }>
}

export function updateAllMusicPlugins(): Promise<MusicPluginUpdateAllResult> {
  return invokeEnvelope('music_plugin_update_all', {})
}

export function setMusicPluginEnabled(
  platform: string,
  enabled: boolean,
): Promise<{ enabled: boolean }> {
  return invokeEnvelope('music_plugin_set_enabled', { platform, enabled })
}

/** 插件声明的用户变量（登录 token / cookie 等）。 */
export interface MusicPluginVariable {
  key: string
  name?: string
  hint?: string
}

export function getMusicPluginVariables(platform: string): Promise<MusicPluginVariable[]> {
  return invokeEnvelope('music_plugin_user_variables', { platform })
}

export function getMusicPluginVariableValues(platform: string): Promise<Record<string, string>> {
  return invokeEnvelope('music_plugin_variables', { platform })
}

export function setMusicPluginVariables(
  platform: string,
  values: Record<string, string>,
): Promise<{ saved: boolean }> {
  return invokeEnvelope('music_plugin_set_variables', { req: { platform, values } })
}

export function searchMusic(params: {
  platform: string
  keyword: string
  page?: number
  searchType?: string
}): Promise<MusicSearchPage> {
  return invokeEnvelope('music_search', {
    req: {
      platform: params.platform,
      keyword: params.keyword,
      page: params.page ?? 1,
      searchType: params.searchType ?? 'music',
    },
  })
}

export function searchMusicCollections(params: {
  platform: string
  keyword: string
  page?: number
  searchType: Exclude<MusicSearchType, 'music'>
}): Promise<MusicCollectionPage> {
  return invokeEnvelope('music_search_collections', {
    req: {
      platform: params.platform,
      keyword: params.keyword,
      page: params.page ?? 1,
      searchType: params.searchType,
    },
  })
}

export function getMusicTopLists(platform: string): Promise<MusicSheetGroup[]> {
  return invokeEnvelope('music_top_lists', { platform })
}

export function getMusicRecommendTags(platform: string): Promise<MusicRecommendTags> {
  return invokeEnvelope('music_recommend_sheet_tags', { platform })
}

export function getMusicRecommendSheets(params: {
  platform: string
  tag: MusicTag
  page?: number
}): Promise<MusicCollectionPage> {
  return invokeEnvelope('music_recommend_sheets', {
    req: { platform: params.platform, tag: params.tag, page: params.page ?? 1 },
  })
}

export function importMusicSheet(platform: string, url: string): Promise<MusicItem[]> {
  return invokeEnvelope('music_import_sheet', { req: { platform, url } })
}

export function importMusicItem(platform: string, url: string): Promise<MusicItem> {
  return invokeEnvelope('music_import_item', { req: { platform, url } })
}

export interface MusicCacheStats {
  bytes: number
  files: number
  limitBytes: number
}

export function getMusicCacheStats(): Promise<MusicCacheStats> {
  return invokeEnvelope('music_cache_stats', {})
}

export function clearMusicCache(): Promise<{ freedBytes: number }> {
  return invokeEnvelope('music_cache_clear', {})
}

export function getMusicCollectionDetail(params: {
  platform: string
  kind: MusicCollectionKind
  item: MusicCollectionItem
  page?: number
  workType?: 'music' | 'album'
}): Promise<MusicCollectionDetail> {
  return invokeEnvelope('music_collection_detail', {
    req: {
      platform: params.platform,
      kind: params.kind,
      item: params.item,
      page: params.page ?? 1,
      workType: params.workType ?? 'music',
    },
  })
}

export function getMusicMediaSource(params: {
  platform: string
  item: MusicItem
  quality?: string
}): Promise<MusicMediaSource> {
  return invokeEnvelope('music_get_media_source', {
    req: {
      platform: params.platform,
      item: params.item,
      quality: params.quality ?? 'standard',
    },
  })
}

export function getMusicLyric(params: {
  platform: string
  item: MusicItem
}): Promise<MusicLyric> {
  return invokeEnvelope('music_get_lyric', {
    req: { platform: params.platform, item: params.item },
  })
}

// ── 收藏 ──

export function toggleMusicFavorite(params: {
  platform: string
  item: MusicItem
}): Promise<{ favorited: boolean }> {
  return invokeEnvelope('music_favorite_toggle', {
    req: { platform: params.platform, item: params.item },
  })
}

export function getMusicFavorites(): Promise<MusicItem[]> {
  return invokeEnvelope('music_favorite_list', {})
}

export function checkMusicFavorite(
  platform: string,
  itemId: string,
): Promise<{ favorited: boolean }> {
  return invokeEnvelope('music_favorite_check', { platform, itemId })
}

// ── 歌单 ──

export interface MusicPlaylistSummary {
  id: string
  name: string
  itemCount: number
  createdAt: number
  updatedAt: number
}

export interface MusicHistoryEntry {
  item: MusicItem
  startedAt: number
  playedSeconds: number
  finished: boolean
}

export interface MusicDailyStat {
  day: string
  seconds: number
  plays: number
}

export interface MusicStatsOverview {
  totalSeconds: number
  todaySeconds: number
  totalPlays: number
  daily: MusicDailyStat[]
}

export function createMusicPlaylist(name: string): Promise<MusicPlaylistSummary> {
  return invokeEnvelope('music_playlist_create', { req: { name } })
}

export function renameMusicPlaylist(id: string, name: string): Promise<{ renamed: boolean }> {
  return invokeEnvelope('music_playlist_rename', { req: { id, name } })
}

export function deleteMusicPlaylist(id: string): Promise<{ deleted: boolean }> {
  return invokeEnvelope('music_playlist_delete', { id })
}

export function getMusicPlaylists(): Promise<MusicPlaylistSummary[]> {
  return invokeEnvelope('music_playlist_list', {})
}

export function getMusicPlaylistItems(playlistId: string): Promise<MusicItem[]> {
  return invokeEnvelope('music_playlist_items', { playlistId })
}

export function addMusicToPlaylist(
  playlistId: string,
  items: MusicItem[],
): Promise<{ added: number }> {
  return invokeEnvelope('music_playlist_add', { req: { playlistId, items } })
}

export function removeMusicFromPlaylist(
  playlistId: string,
  platform: string,
  itemId: string,
): Promise<{ removed: boolean }> {
  return invokeEnvelope('music_playlist_remove', {
    req: { playlistId, platform, itemId },
  })
}

export function removeMusicFromPlaylistBatch(
  playlistId: string,
  items: MusicItem[],
): Promise<{ removed: number }> {
  return invokeEnvelope('music_playlist_remove_batch', {
    req: {
      playlistId,
      keys: items.map((item) => ({ platform: item.platform, itemId: item.id })),
    },
  })
}

export function reorderMusicPlaylist(
  playlistId: string,
  keys: Array<{ platform: string; itemId: string }>,
): Promise<{ reordered: boolean }> {
  return invokeEnvelope('music_playlist_reorder', { req: { playlistId, keys } })
}

// ── 历史与统计 ──

export function recordMusicHistory(params: {
  platform: string
  item: MusicItem
  seconds: number
  finished?: boolean
}): Promise<{ saved: boolean }> {
  return invokeEnvelope('music_history_record', {
    req: {
      platform: params.platform,
      item: params.item,
      seconds: params.seconds,
      finished: params.finished ?? false,
    },
  })
}

export function getMusicHistory(limit = 100): Promise<MusicHistoryEntry[]> {
  return invokeEnvelope('music_history_list', { limit })
}

export function clearMusicHistory(): Promise<{ removed: number }> {
  return invokeEnvelope('music_history_clear', {})
}

export function getMusicStatsOverview(days = 30): Promise<MusicStatsOverview> {
  return invokeEnvelope('music_stats_overview', { days })
}

// ── 本地音乐 ──

export interface LocalTrack {
  path: string
  title: string
  artist: string
  album: string
  durationMs: number
  format: string
  size: number
  mtime: number
  addedAt: number
}

/** 打开系统文件选择框并导入；取消时返回空数组。 */
export function pickLocalMusic(): Promise<LocalTrack[]> {
  return invokeEnvelope('music_local_pick', {})
}

export function importLocalMusic(paths: string[]): Promise<LocalTrack[]> {
  return invokeEnvelope('music_local_import', { req: { paths } })
}

export function getLocalMusic(): Promise<LocalTrack[]> {
  return invokeEnvelope('music_local_list', {})
}

export function removeLocalMusic(path: string): Promise<{ removed: boolean }> {
  return invokeEnvelope('music_local_remove', { path })
}
