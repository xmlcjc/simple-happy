<template>
  <div class="music-pane">
    <div class="pane-toolbar">
      <span class="pane-count">{{ libraryStore.localTracks.length }} 首本地歌曲</span>
      <button type="button" class="import-btn" :disabled="importing" @click="importFiles">
        {{ importing ? '正在导入…' : '导入本地音乐' }}
      </button>
    </div>

    <MusicTrackList
      :items="items"
      empty-text="还没有导入本地音乐"
      empty-hint="支持 mp3 / flac / m4a / aac / ogg / wav。"
      @play="emit('play', $event, items)"
    >
      <template #actions="{ item }">
        <button type="button" class="row-action" title="从曲库移除" @click.stop="removeTrack(item)">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M5 12h14" />
          </svg>
        </button>
      </template>
    </MusicTrackList>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useMusicLibraryStore } from '../../stores/musicLibrary'
import { useAppStore } from '../../stores/app'
import MusicTrackList from './MusicTrackList.vue'
import type { MusicItem } from '../../api/music'

const emit = defineEmits<{
  play: [item: MusicItem, queue: MusicItem[]]
}>()

const libraryStore = useMusicLibraryStore()
const appStore = useAppStore()
const importing = ref(false)

const items = computed<MusicItem[]>(() =>
  libraryStore.localTracks.map((track) => ({
    platform: 'local',
    id: track.path,
    title: track.title || track.path,
    artist: track.artist,
    album: track.album,
    duration: track.durationMs > 0 ? track.durationMs / 1000 : undefined,
    localPath: track.path,
  })),
)

async function importFiles() {
  importing.value = true
  try {
    const count = await libraryStore.pickLocal()
    appStore.showToast(count > 0 ? `已导入 ${count} 首本地歌曲` : '没有选择可导入的音频文件', count > 0 ? 'success' : 'warning')
  } catch (error) {
    appStore.showToast((error as Error).message || '导入失败', 'error')
  } finally {
    importing.value = false
  }
}

async function removeTrack(item: MusicItem) {
  const path = String(item.localPath || item.id)
  const confirmed = await appStore.confirmDialog(
    `从本地曲库移除「${item.title}」？\n只会移除记录，不会删除磁盘上的文件。`,
    { title: '移除本地音乐', danger: true },
  )
  if (!confirmed) return
  try {
    await libraryStore.removeLocal(path)
    appStore.showToast('已从曲库移除', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '移除失败', 'error')
  }
}

onMounted(() => {
  void libraryStore.loadLocal().catch(() => undefined)
})
</script>

<style scoped>
.music-pane {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  overflow: hidden;
}

.pane-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.pane-count {
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.import-btn {
  padding: 7px 16px;
  border-radius: var(--radius-md);
  background: var(--color-primary-soft-bg);
  color: var(--color-primary-soft-text);
  font-size: var(--text-sm);
  font-weight: 600;
}

.import-btn:disabled {
  opacity: 0.55;
  cursor: wait;
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
