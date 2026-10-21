<template>
  <div class="playlist-layout">
    <aside class="playlist-side">
      <form class="playlist-create" @submit.prevent="createPlaylist">
        <input v-model.trim="newName" type="text" placeholder="新建歌单" maxlength="40" />
        <button type="submit" :disabled="!newName || creating">+</button>
      </form>

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
          <button type="button" class="ghost-btn" @click="startRename">重命名</button>
          <button type="button" class="ghost-btn danger" @click="removePlaylist">删除</button>
        </header>

        <form v-if="renaming" class="rename-row" @submit.prevent="applyRename">
          <input v-model.trim="renameText" type="text" maxlength="40" />
          <button type="submit" :disabled="!renameText">保存</button>
          <button type="button" @click="renaming = false">取消</button>
        </form>

        <MusicTrackList
          :items="libraryStore.playlistItems"
          empty-text="歌单还是空的"
          empty-hint="在搜索结果或收藏里点「+」加入。"
          @play="emit('play', $event, libraryStore.playlistItems)"
        >
          <template #actions="{ item }">
            <button type="button" class="row-action" title="从歌单移除" @click.stop="removeItem(item)">
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
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useMusicLibraryStore } from '../../stores/musicLibrary'
import { useAppStore } from '../../stores/app'
import { renameMusicPlaylist } from '../../api/music'
import MusicTrackList from './MusicTrackList.vue'
import type { MusicItem } from '../../api/music'

const emit = defineEmits<{
  play: [item: MusicItem, queue: MusicItem[]]
}>()

const libraryStore = useMusicLibraryStore()
const appStore = useAppStore()

const newName = ref('')
const creating = ref(false)
const renaming = ref(false)
const renameText = ref('')

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
