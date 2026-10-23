<template>
  <div class="recent-view">
    <div class="recent-content">
      <div class="recent-header">
        <h1 class="recent-title">
          阅读历史
          <span class="recent-count">({{ filteredRecentBooks.length }})</span>
        </h1>
        <div class="recent-actions">
          <button
            class="recent-clear-btn"
            :disabled="!shelfStore.recentBooks.length"
            @click="handleClearRecent"
          >
            一键清空
          </button>
        </div>
      </div>

      <div class="history-stats">
        <div class="stat-card">
          <strong>{{ formatDuration(stats.todaySeconds) }}</strong>
          <small>今日阅读</small>
        </div>
        <div class="stat-card">
          <strong>{{ formatDuration(stats.monthSeconds) }}</strong>
          <small>近 30 天</small>
        </div>
        <div class="stat-card">
          <strong>{{ formatDuration(stats.totalSeconds) }}</strong>
          <small>累计阅读</small>
        </div>
        <div class="stat-card">
          <strong>{{ formatNumber(stats.totalCharacters) }}</strong>
          <small>累计字数</small>
        </div>
      </div>

      <ReadingHeatmap :daily="heatmapDays" :weeks="16" />

      <div class="recent-search-row">
        <input
          v-model.trim="searchText"
          class="recent-search-input"
          placeholder="搜索最近阅读"
        />
      </div>

      <BookGrid
        :books="filteredRecentBooks"
        :loading="shelfStore.loading"
        empty-text="暂无最近阅读"
        :show-delete-action="true"
        @click="handleBookClick"
        @info="handleBookInfo"
        @delete="handleRecentDelete"
        @contextmenu="handleBookContextMenu"
      />
    </div>

    <BookDetailModal
      v-model="showDetail"
      :book="selectedBook"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import BookDetailModal from '../components/BookDetailModal.vue'
import BookGrid from '../components/BookGrid.vue'
import ReadingHeatmap from '../components/ReadingHeatmap.vue'
import { showContextMenu } from '../composables/useContextMenu'
import { useBookshelfStore } from '../stores/bookshelf'
import { useReaderStore } from '../stores/reader'
import { getReadingStatsDaily, getReadingStatsSummary } from '../api/readingStats'
import { recordRssReadFromBook } from '../utils/rssReadHistory'
import type { Book, SearchBook } from '../types'

const router = useRouter()
const shelfStore = useBookshelfStore()
const readerStore = useReaderStore()

const showDetail = ref(false)
const selectedBook = ref<Book | SearchBook | null>(null)
const openingBookUrl = ref('')
const searchText = ref('')

const stats = ref({
  todaySeconds: 0,
  monthSeconds: 0,
  totalSeconds: 0,
  totalCharacters: 0,
})

/** 热力图取最近 16 周（112 天）的按日数据；总览卡片仍只看近 30 天。 */
const heatmapDays = ref<Array<{ date: string; seconds: number }>>([])

function dateStr(offsetDays: number) {
  const date = new Date()
  date.setDate(date.getDate() - offsetDays)
  return date.toISOString().slice(0, 10)
}

async function loadStats() {
  const end = dateStr(0)
  const start = dateStr(111)
  const monthStart = dateStr(29)
  const [daily, summary] = await Promise.all([
    getReadingStatsDaily(start, end),
    getReadingStatsSummary(),
  ])
  heatmapDays.value = daily
  stats.value = {
    todaySeconds: daily.find((item) => item.date === end)?.seconds ?? 0,
    monthSeconds: daily
      .filter((item) => item.date >= monthStart)
      .reduce((total, item) => total + item.seconds, 0),
    totalSeconds: summary.totalSeconds,
    totalCharacters: summary.totalCharacters,
  }
}

function formatDuration(seconds: number) {
  if (seconds <= 0) return '0分钟'
  if (seconds < 60) return '少于1分钟'
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  return hours > 0 ? `${hours}小时${rest}分` : `${Math.max(1, minutes)}分钟`
}

function formatNumber(value: number) {
  return value >= 10000 ? `${(value / 10000).toFixed(1)}万` : String(value)
}

const filteredRecentBooks = computed(() => {
  // RSS 文章有独立的历史页与统计，这里只展示书籍。
  const list = shelfStore.recentBooks.filter((book) => book.recentKind !== 'rss')

  const keyword = searchText.value.trim().toLowerCase()
  if (!keyword) return list

  return list.filter((book) =>
    [
      book.name,
      book.author,
      book.intro,
      book.originName,
      book.origin,
      book.latestChapterTitle,
      book.durChapterTitle,
    ]
      .filter(Boolean)
      .some((value) => String(value).toLowerCase().includes(keyword)),
  )
})

onMounted(async () => {
  await shelfStore.fetchBooks().catch(() => undefined)
  await shelfStore.refreshRecentBooks().catch(() => undefined)
  // 旧版本的 RSS 记录只存在「最近阅读」里，迁移到 RSS 独立历史，避免统计丢失。
  for (const book of shelfStore.recentBooks) recordRssReadFromBook(book)
  await loadStats().catch(() => undefined)
})

async function handleBookClick(book: Book | SearchBook) {
  const currentBook = book as Book
  if (currentBook.recentKind === 'rss' && currentBook.rssSourceUrl && (currentBook.rssLink || currentBook.bookUrl)) {
    await router.push({
      name: 'rss-article',
      query: {
        source: currentBook.rssSourceUrl,
        link: currentBook.rssLink || currentBook.bookUrl,
        title: currentBook.name || '',
        pubDate: currentBook.rssPubDate || '',
        origin: currentBook.author || '',
      },
    })
    return
  }
  if (!currentBook.origin || !currentBook.bookUrl) return
  if (openingBookUrl.value === currentBook.bookUrl) return

  openingBookUrl.value = currentBook.bookUrl
  try {
    await shelfStore.moveBookToFront(currentBook.bookUrl).catch(() => undefined)
    const loadBookTask = readerStore.loadBook(currentBook)
    await router.push('/reader')
    await loadBookTask
  } finally {
    openingBookUrl.value = ''
  }
}

function handleBookInfo(book: Book | SearchBook) {
  const currentBook = book as Book
  if (currentBook.recentKind === 'rss') {
    handleBookClick(book)
    return
  }
  selectedBook.value = book
  showDetail.value = true
}

async function handleRecentDelete(book: Book | SearchBook) {
  await shelfStore.removeRecentBook(book as Book).catch(() => undefined)
}

function handleBookContextMenu({ book, event }: { book: Book | SearchBook; event: MouseEvent }) {
  const currentBook = book as Book
  const isRss = currentBook.recentKind === 'rss'
  const menuItems = [
    { label: isRss ? '打开文章' : '开始阅读', action: () => handleBookClick(book) },
    { label: '查看详情', action: () => handleBookInfo(book) },
    { divider: true },
    { label: '从最近移除', danger: true, action: () => handleRecentDelete(book) },
  ]
  showContextMenu(event, menuItems, book)
}

async function handleClearRecent() {
  await shelfStore.clearAllRecentBooks().catch(() => undefined)
}
</script>

<style scoped>
.recent-view {
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

.recent-content {
  height: 100%;
  padding: 0 var(--space-6);
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.recent-header {
  padding: var(--space-6) 0 var(--space-3);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  flex-shrink: 0;
}

/* 历史页顶部的统计卡片，规格与音乐历史页一致。 */
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

.recent-content :deep(.book-grid) {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

.recent-search-row {
  padding-bottom: var(--space-3);
  flex-shrink: 0;
}

.recent-search-input {
  width: 100%;
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  border-radius: var(--radius-lg);
  padding: 12px 14px;
  font-size: var(--text-sm);
  color: var(--color-text);
}

.recent-search-input::placeholder {
  color: var(--color-text-tertiary);
}

.recent-title {
  font-size: var(--text-2xl);
  font-weight: 700;
  letter-spacing: 0;
}

.recent-count {
  font-size: var(--text-base);
  font-weight: 400;
  color: var(--color-text-tertiary);
}

.recent-actions {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.recent-filters {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.filter-chip {
  padding: 8px 14px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  transition: all var(--duration-fast) var(--ease-out);
}

.filter-chip.active {
  border-color: rgba(201, 127, 58, 0.26);
  background: rgba(201, 127, 58, 0.1);
  color: var(--color-primary);
}

.recent-clear-btn {
  padding: 10px 16px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  transition: all var(--duration-fast) var(--ease-out);
}

.recent-clear-btn:hover:not(:disabled) {
  border-color: rgba(225, 76, 76, 0.22);
  color: var(--color-danger);
}

.recent-clear-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

@media (max-width: 640px) {
  .recent-header {
    align-items: flex-start;
    flex-direction: column;
  }

  .recent-actions {
    width: 100%;
    justify-content: space-between;
  }
}
</style>
