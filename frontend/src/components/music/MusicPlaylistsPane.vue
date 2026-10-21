<template>
  <div class="playlist-layout">
    <aside class="playlist-side">
      <form class="playlist-create" @submit.prevent="createPlaylist">
        <input v-model.trim="newName" type="text" placeholder="新建歌单" maxlength="40" />
        <button type="submit" :disabled="!newName || creating">+</button>
      </form>

      <button type="button" class="import-entry" @click="importOpen = true">链接导入</button>

      <div class="playlist-rows">
        <button
          v-for="playlist in libraryStore.playlists"
          :key="playlist.id"
          type="button"
          class="playlist-row"
          :class="{ active: libraryStore.activePlaylistId === playlist.id }"
          @click="selectPlaylist(playlist.id)"
        >
          <span class="playlist-name">{{ playlist.name }}</span>
          <small>{{ playlist.itemCount }}</small>
        </button>
        <p v-if="!libraryStore.playlists.length" class="playlist-empty">还没有歌单</p>
      </div>
    </aside>

    <section class="playlist-main">
      <template v-if="activePlaylist">
        <header class="playlist-head">
          <strong>{{ activePlaylist.name }}</strong>
          <small>{{ libraryStore.playlistItems.length }} 首</small>
          <span class="playlist-spacer"></span>
          <select v-model="sortKey" class="sort-select" title="歌曲排序">
            <option value="added">默认顺序</option>
            <option value="title">按标题</option>
            <option value="artist">按歌手</option>
            <option value="album">按专辑</option>
          </select>
          <button type="button" class="ghost-btn" @click="toggleEdit">
            {{ editing ? '完成' : '编辑' }}
          </button>
          <button type="button" class="ghost-btn" @click="startRename">重命名</button>
          <button type="button" class="ghost-btn danger" @click="removePlaylist">删除</button>
        </header>

        <div v-if="editing" class="batch-bar">
          <button type="button" @click="selectAll">全选</button>
          <button type="button" @click="invertSelection">反选</button>
          <span class="batch-count">已选 {{ selected.length }} 首</span>
          <span class="playlist-spacer"></span>
          <button type="button" :disabled="!selected.length" @click="addSelectedToPlaylist">
            加入其它歌单
          </button>
          <button type="button" class="danger" :disabled="!selected.length" @click="removeSelected">
            移出歌单
          </button>
        </div>

        <form v-if="renaming" class="rename-row" @submit.prevent="applyRename">
          <input v-model.trim="renameText" type="text" maxlength="40" />
          <button type="submit" :disabled="!renameText">保存</button>
          <button type="button" @click="renaming = false">取消</button>
        </form>

        <MusicTrackList
          :items="displayedItems"
          empty-text="歌单还是空的"
          empty-hint="在搜索结果或收藏里点「+」加入。"
          @play="handleRowPlay"
          @play-next="emit('play-next', $event)"
        >
          <template #actions="{ item }">
            <button
              v-if="editing"
              type="button"
              class="select-box"
              :class="{ checked: isSelected(item) }"
              :aria-label="isSelected(item) ? '取消选择' : '选择'"
              @click.stop="toggleSelect(item)"
            >
              <svg v-if="isSelected(item)" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
                <path d="m5 13 4 4L19 7" />
              </svg>
            </button>
            <button
              v-else
              type="button"
              class="row-action"
              title="从歌单移除"
              @click.stop="removeItem(item)"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M5 12h14" />
              </svg>
            </button>
          </template>
        </MusicTrackList>
      </template>

      <div v-else class="playlist-placeholder">
        <p>选择左侧歌单查看歌曲</p>
        <small>歌单只保存在本地，不会上传。</small>
      </div>
    </section>
  </div>

  <MusicImportDialog v-model="importOpen" @imported="handleImported" />
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useMusicLibraryStore } from '../../stores/musicLibrary'
import { useAppStore } from '../../stores/app'
import { renameMusicPlaylist } from '../../api/music'
import MusicTrackList from './MusicTrackList.vue'
import MusicImportDialog from './MusicImportDialog.vue'
import type { MusicItem } from '../../api/music'

const emit = defineEmits<{
  play: [item: MusicItem, queue: MusicItem[]]
  'play-next': [item: MusicItem]
  'import-items': [items: MusicItem[]]
}>()

const libraryStore = useMusicLibraryStore()
const appStore = useAppStore()

const newName = ref('')
const creating = ref(false)
const renaming = ref(false)
const renameText = ref('')
const importOpen = ref(false)

type SortKey = 'added' | 'title' | 'artist' | 'album'
const sortKey = ref<SortKey>('added')
const editing = ref(false)
const selected = ref<string[]>([])

function keyOf(item: MusicItem) {
  return `${item.platform}:${item.id}`
}

/** 默认按歌单内顺序；其余排序只影响展示，不写库。 */
const displayedItems = computed(() => {
  const list = [...libraryStore.playlistItems]
  if (sortKey.value === 'added') return list
  const key = sortKey.value
  return list.sort((a, b) =>
    String(a[key] ?? '').localeCompare(String(b[key] ?? ''), 'zh-Hans-CN'),
  )
})

function isSelected(item: MusicItem) {
  return selected.value.includes(keyOf(item))
}

function toggleSelect(item: MusicItem) {
  const key = keyOf(item)
  selected.value = selected.value.includes(key)
    ? selected.value.filter((entry) => entry !== key)
    : [...selected.value, key]
}

function toggleEdit() {
  editing.value = !editing.value
  if (!editing.value) selected.value = []
}

function selectAll() {
  selected.value = displayedItems.value.map(keyOf)
}

function invertSelection() {
  selected.value = displayedItems.value
    .map(keyOf)
    .filter((key) => !selected.value.includes(key))
}

function handleRowPlay(item: MusicItem) {
  if (editing.value) {
    toggleSelect(item)
    return
  }
  emit('play', item, displayedItems.value)
}

function addSelectedToPlaylist() {
  const items = displayedItems.value.filter(isSelected)
  if (!items.length) return
  emit('import-items', items)
  selected.value = []
  editing.value = false
}

async function removeSelected() {
  if (!activePlaylist.value) return
  const items = displayedItems.value.filter(isSelected)
  if (!items.length) return
  const confirmed = await appStore.confirmDialog(
    `从歌单「${activePlaylist.value.name}」移除选中的 ${items.length} 首歌曲？`,
    { title: '批量移除', danger: true },
  )
  if (!confirmed) return
  try {
    const removed = await libraryStore.removeManyFromPlaylist(activePlaylist.value.id, items)
    appStore.showToast(`已移除 ${removed} 首`, 'success')
    selected.value = []
    editing.value = false
  } catch (error) {
    appStore.showToast((error as Error).message || '移除失败', 'error')
  }
}

/** 导入对话框只负责解析，存到哪个歌单交给统一的选择器。 */
function handleImported(items: MusicItem[]) {
  if (items.length) emit('import-items', items)
}

const activePlaylist = computed(
  () => libraryStore.playlists.find((item) => item.id === libraryStore.activePlaylistId) ?? null,
)

async function refresh() {
  try {
    await libraryStore.loadPlaylists()
    const first = libraryStore.playlists[0]
    if (first && !libraryStore.activePlaylistId) await libraryStore.loadPlaylistItems(first.id)
    else if (libraryStore.activePlaylistId) {
      await libraryStore.loadPlaylistItems(libraryStore.activePlaylistId)
    }
  } catch (error) {
    appStore.showToast((error as Error).message || '加载歌单失败', 'error')
  }
}

async function createPlaylist() {
  if (!newName.value) return
  creating.value = true
  try {
    const playlist = await libraryStore.createPlaylist(newName.value)
    newName.value = ''
    await libraryStore.loadPlaylistItems(playlist.id)
  } catch (error) {
    appStore.showToast((error as Error).message || '新建歌单失败', 'error')
  } finally {
    creating.value = false
  }
}

async function selectPlaylist(id: string) {
  editing.value = false
  selected.value = []
  try {
    await libraryStore.loadPlaylistItems(id)
  } catch (error) {
    appStore.showToast((error as Error).message || '加载歌单失败', 'error')
  }
}

function startRename() {
  if (!activePlaylist.value) return
  renameText.value = activePlaylist.value.name
  renaming.value = true
}

async function applyRename() {
  if (!activePlaylist.value || !renameText.value) return
  try {
    await renameMusicPlaylist(activePlaylist.value.id, renameText.value)
    renaming.value = false
    await libraryStore.loadPlaylists()
    appStore.showToast('歌单已重命名', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '重命名失败', 'error')
  }
}

async function removePlaylist() {
  if (!activePlaylist.value) return
  const confirmed = await appStore.confirmDialog(
    `删除歌单「${activePlaylist.value.name}」？歌单中的歌曲不会被删除。`,
    { title: '删除歌单', danger: true },
  )
  if (!confirmed) return
  try {
    await libraryStore.deletePlaylist(activePlaylist.value.id)
    appStore.showToast('歌单已删除', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '删除失败', 'error')
  }
}

async function removeItem(item: MusicItem) {
  if (!activePlaylist.value) return
  try {
    await libraryStore.removeFromPlaylist(activePlaylist.value.id, item)
  } catch (error) {
    appStore.showToast((error as Error).message || '移除失败', 'error')
  }
}

onMounted(refresh)
</script>

<style scoped>
.playlist-layout {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 190px minmax(0, 1fr);
  gap: var(--space-4);
  overflow: hidden;
}

.playlist-side {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  border-right: 1px solid var(--color-divider);
  padding-right: var(--space-3);
}

.playlist-create {
  display: flex;
  gap: 4px;
}

.playlist-create input {
  flex: 1;
  min-width: 0;
  padding: 6px 8px;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-bg-elevated);
  color: var(--color-text);
  font-size: 12px;
  outline: none;
}

.playlist-create input:focus {
  border-color: var(--color-primary);
}

.playlist-create button {
  width: 28px;
  border-radius: var(--radius-md);
  background: var(--color-primary-soft-bg);
  color: var(--color-primary-soft-text);
  font-size: 16px;
}

.playlist-create button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.import-entry {
  width: 100%;
  padding: 6px 8px;
  border-radius: var(--radius-md);
  border: 1px dashed var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.import-entry:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.playlist-rows {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.playlist-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 7px 8px;
  border-radius: var(--radius-md);
  text-align: left;
  color: var(--color-text-secondary);
}

.playlist-row:hover {
  background: var(--color-bg-hover);
}

.playlist-row.active {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.playlist-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
}

.playlist-row small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.playlist-empty {
  margin: var(--space-3) 0;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-align: center;
}

.playlist-main {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  overflow: hidden;
}

.playlist-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.playlist-head strong {
  color: var(--color-text);
  font-size: 14px;
}

.playlist-head small {
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.playlist-spacer {
  flex: 1;
}

.sort-select {
  padding: 4px 6px;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  color: var(--color-text-secondary);
  font-size: 11px;
  outline: none;
}

.sort-select:focus {
  border-color: var(--color-primary);
}

.batch-bar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 6px 10px;
  border-radius: var(--radius-md);
  background: var(--color-bg-sunken);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.batch-bar button {
  padding: 4px 10px;
  border-radius: var(--radius-md);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.batch-bar button:hover:not(:disabled) {
  background: var(--color-bg-hover);
  color: var(--color-primary);
}

.batch-bar button.danger:hover:not(:disabled) {
  color: var(--color-danger, #c0392b);
}

.batch-bar button:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.batch-count {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.select-box {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  margin-right: 4px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  color: var(--color-primary-soft-text);
}

.select-box.checked {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: #fff;
}

.select-box svg {
  width: 13px;
  height: 13px;
}

.ghost-btn {
  padding: 4px 10px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.ghost-btn:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.ghost-btn.danger:hover {
  border-color: var(--color-danger, #c0392b);
  color: var(--color-danger, #c0392b);
}

.rename-row {
  display: flex;
  gap: var(--space-2);
}

.rename-row input {
  flex: 1;
  min-width: 0;
  padding: 6px 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg-elevated);
  color: var(--color-text);
  font-size: 13px;
  outline: none;
}

.rename-row button {
  flex-shrink: 0;
  padding: 6px 12px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.playlist-placeholder {
  margin: auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  color: var(--color-text-tertiary);
  font-size: 13px;
  text-align: center;
}

.playlist-placeholder p {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: 14px;
}

.row-action {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
}

.row-action svg {
  width: 15px;
  height: 15px;
}

.row-action:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}
</style>
