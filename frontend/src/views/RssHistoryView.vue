<template>
  <div class="rss-history-view">
    <div class="rss-history-content">
      <header class="history-header">
        <h1 class="history-title">
          RSS 阅读历史
          <span class="history-count">({{ entries.length }})</span>
        </h1>
        <div class="history-actions">
          <button type="button" class="ghost-btn" @click="router.push('/rss')">返回 RSS</button>
          <button
            type="button"
            class="ghost-btn danger"
            :disabled="!entries.length"
            @click="handleClear"
          >清空历史</button>
        </div>
      </header>

      <div class="history-stats">
        <div class="stat-card">
          <strong>{{ stats.today }}</strong>
          <small>今日阅读</small>
        </div>
        <div class="stat-card">
          <strong>{{ stats.month }}</strong>
          <small>近 30 天</small>
        </div>
        <div class="stat-card">
          <strong>{{ stats.total }}</strong>
          <small>累计篇数</small>
        </div>
        <div class="stat-card">
          <strong>{{ sourceCount }}</strong>
          <small>阅读来源</small>
        </div>
      </div>

      <div v-if="!entries.length" class="history-empty">
        <p>还没有 RSS 阅读记录</p>
        <small>在 RSS 页打开文章后会记录在这里</small>
      </div>
      <div v-else class="entry-list">
        <div v-for="entry in entries" :key="entry.key" class="entry-row">
          <button type="button" class="entry-main" @click="openEntry(entry)">
            <span class="entry-text">
              <strong :title="entry.title">{{ entry.title }}</strong>
              <small :title="entry.sourceUrl">{{ sourceName(entry.sourceUrl) }} · {{ formatTime(entry.readAt) }}</small>
            </span>
            <span class="entry-arrow">›</span>
          </button>
          <button
            type="button"
            class="entry-remove"
            title="删除这条记录"
            @click="removeEntry(entry)"
          >×</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAppStore } from '../stores/app'
import {
  clearRssReadHistory,
  loadRssReadHistory,
  removeRssRead,
  summarizeRssRead,
  type RssReadEntry,
} from '../utils/rssReadHistory'

const router = useRouter()
const appStore = useAppStore()

const entries = ref<RssReadEntry[]>([])
const stats = computed(() => summarizeRssRead(entries.value))
const sourceCount = computed(
  () => new Set(entries.value.map((entry) => entry.sourceUrl)).size,
)

function reload() {
  entries.value = loadRssReadHistory()
}

onMounted(reload)

function sourceName(sourceUrl: string) {
  try {
    return new URL(sourceUrl).hostname.replace(/^www\./, '')
  } catch {
    return sourceUrl
  }
}

function formatTime(timestamp: number) {
  const date = new Date(timestamp)
  const now = new Date()
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate()
  const hh = String(date.getHours()).padStart(2, '0')
  const mm = String(date.getMinutes()).padStart(2, '0')
  if (sameDay) return `今天 ${hh}:${mm}`
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${month}-${day} ${hh}:${mm}`
}

function openEntry(entry: RssReadEntry) {
  void router.push({
    name: 'rss-article',
    query: {
      source: entry.sourceUrl,
      link: entry.link,
      title: entry.title,
      pubDate: entry.pubDate,
    },
  })
}

function removeEntry(entry: RssReadEntry) {
  removeRssRead(entry.key)
  reload()
}

async function handleClear() {
  const confirmed = await appStore.confirmDialog(
    `清空全部 ${entries.value.length} 条 RSS 阅读记录？此操作不可恢复。`,
    { title: '清空 RSS 阅读历史', danger: true },
  )
  if (!confirmed) return
  clearRssReadHistory()
  reload()
}
</script>

<style scoped>
.rss-history-view {
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

.rss-history-content {
  height: 100%;
  padding: 0 var(--space-6);
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.history-header {
  padding: var(--space-6) 0 var(--space-3);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  flex-shrink: 0;
}

.history-title {
  margin: 0;
  color: var(--color-text);
  font-size: 22px;
  font-weight: 600;
}

.history-count {
  color: var(--color-text-tertiary);
  font-size: 13px;
  font-weight: 400;
}

.history-actions {
  display: flex;
  gap: var(--space-2);
}

.ghost-btn {
  padding: 6px 14px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.ghost-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.ghost-btn.danger:hover:not(:disabled) {
  border-color: var(--color-danger-border, rgba(192, 57, 43, 0.45));
  color: var(--color-danger, #c0392b);
}

.ghost-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.history-stats {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  padding-bottom: var(--space-3);
  flex-shrink: 0;
}

.stat-card {
  min-width: 120px;
  padding: 10px 14px;
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.stat-card strong {
  color: var(--color-text);
  font-size: 17px;
  font-weight: 600;
}

.stat-card small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.entry-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-bottom: var(--space-4);
}

.entry-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  border-radius: var(--radius-md);
}

.entry-row:hover {
  background: var(--color-bg-elevated);
}

.entry-main {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 10px 12px;
  text-align: left;
}

.entry-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.entry-text strong,
.entry-text small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.entry-text strong {
  color: var(--color-text);
  font-size: 13px;
  font-weight: 500;
}

.entry-text small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.entry-arrow {
  color: var(--color-text-tertiary);
  font-size: 16px;
}

.entry-remove {
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  margin-right: 6px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 16px;
  line-height: 1;
}

.entry-remove:hover {
  background: var(--color-bg-hover);
  color: var(--color-danger, #c0392b);
}

.history-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  color: var(--color-text-tertiary);
  font-size: 13px;
}

.history-empty p {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: 14px;
}
</style>
