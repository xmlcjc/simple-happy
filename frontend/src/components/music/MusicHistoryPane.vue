<template>
  <div class="music-pane">
    <div class="stat-cards">
      <div class="stat-card">
        <small>今日听歌</small>
        <strong>{{ formatMinutes(libraryStore.stats?.todaySeconds || 0) }}</strong>
      </div>
      <div class="stat-card">
        <small>近 30 天</small>
        <strong>{{ formatMinutes(libraryStore.stats?.totalSeconds || 0) }}</strong>
      </div>
      <div class="stat-card">
        <small>完整播放</small>
        <strong>{{ libraryStore.stats?.totalPlays || 0 }} 次</strong>
      </div>
      <span class="stat-spacer"></span>
      <button type="button" class="ghost-btn" @click="refresh">刷新</button>
      <button type="button" class="ghost-btn danger" @click="clearAll">清空历史</button>
    </div>

    <div v-if="!libraryStore.history.length" class="history-empty">
      <p>还没有播放记录</p>
      <small>播放任意歌曲后会自动记录到本地。</small>
    </div>

    <div v-else class="history-list">
      <button
        v-for="entry in libraryStore.history"
        :key="`${entry.item.platform}:${entry.item.id}`"
        type="button"
        class="history-row"
        @click="emit('play', entry.item, libraryStore.history.map((row) => row.item))"
      >
        <img
          v-if="entry.item.artwork"
          class="history-cover"
          :src="entry.item.artwork"
          :alt="entry.item.title"
          loading="lazy"
          referrerpolicy="no-referrer"
        />
        <span v-else class="history-cover placeholder">{{ (entry.item.title || '?').slice(0, 1) }}</span>
        <span class="history-text">
          <strong>{{ entry.item.title || '未知标题' }}</strong>
          <small>{{ entry.item.artist || '未知歌手' }} · {{ formatTime(entry.startedAt) }}</small>
        </span>
        <span class="history-duration">{{ formatMinutes(entry.playedSeconds) }}</span>
        <span v-if="entry.finished" class="history-finished">完整</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useMusicLibraryStore } from '../../stores/musicLibrary'
import { useAppStore } from '../../stores/app'
import type { MusicItem } from '../../api/music'

const emit = defineEmits<{
  play: [item: MusicItem, queue: MusicItem[]]
}>()

const libraryStore = useMusicLibraryStore()
const appStore = useAppStore()

function formatMinutes(seconds: number) {
  if (seconds < 60) return `${seconds} 秒`
  const minutes = Math.round(seconds / 60)
  if (minutes < 60) return `${minutes} 分钟`
  return `${(minutes / 60).toFixed(1)} 小时`
}

function formatTime(timestamp: number) {
  const date = new Date(timestamp)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

async function refresh() {
  try {
    await Promise.all([libraryStore.loadHistory(100), libraryStore.loadStats(30)])
  } catch (error) {
    appStore.showToast((error as Error).message || '加载播放记录失败', 'error')
  }
}

async function clearAll() {
  const confirmed = await appStore.confirmDialog('清空全部播放历史？听歌统计不会清零。', {
    title: '清空历史',
    danger: true,
  })
  if (!confirmed) return
  try {
    await libraryStore.clearHistory()
    appStore.showToast('播放历史已清空', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '清空失败', 'error')
  }
}

onMounted(refresh)
</script>

<style scoped>
.music-pane {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  overflow: hidden;
}

.stat-cards {
  display: flex;
  align-items: stretch;
  gap: var(--space-2);
}

.stat-card {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 110px;
  padding: 10px 14px;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
}

.stat-card small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.stat-card strong {
  color: var(--color-text);
  font-size: 16px;
  font-weight: 700;
}

.stat-spacer {
  flex: 1;
}

.ghost-btn {
  align-self: center;
  padding: 5px 12px;
  border-radius: var(--radius-full);
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

.history-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.history-row {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 7px 10px;
  border-radius: var(--radius-md);
  text-align: left;
}

.history-row:hover {
  background: var(--color-bg-hover);
}

.history-cover {
  width: 34px;
  height: 34px;
  border-radius: var(--radius-sm);
  object-fit: cover;
  flex-shrink: 0;
  background: var(--color-bg-sunken);
}

.history-cover.placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
  font-size: 14px;
}

.history-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.history-text strong,
.history-text small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.history-text strong {
  color: var(--color-text);
  font-size: 13px;
  font-weight: 600;
}

.history-text small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.history-duration {
  flex-shrink: 0;
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.history-finished {
  flex-shrink: 0;
  padding: 2px 7px;
  border-radius: var(--radius-sm);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-size: 10px;
}

.history-empty {
  margin: auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  color: var(--color-text-tertiary);
  font-size: 13px;
  text-align: center;
}

.history-empty p {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: 14px;
}
</style>
