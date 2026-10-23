<template>
  <div class="home-view">
    <!-- 左侧功能栏 -->
    <aside class="shelf-rail" aria-label="小说功能">
      <button
        type="button"
        class="rail-item"
        :class="{ active: view === 'shelf' }"
        title="小说"
        @click="view = 'shelf'"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H20" />
          <path d="M4 5.5V19a2 2 0 0 0 2 2h14" />
          <path d="M8 7h8M8 11h8M8 15h5" />
        </svg>
        <span>小说</span>
      </button>

      <button
        type="button"
        class="rail-item"
        :class="{ active: view === 'history' }"
        title="阅读历史"
        @click="view = 'history'"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="9" />
          <path d="M12 7v5l3 2" />
        </svg>
        <span>历史</span>
      </button>

      <button
        type="button"
        class="rail-item"
        title="导入本地书"
        :disabled="localBookUploading"
        @click="triggerLocalBookUpload"
      >
        <svg v-if="!localBookUploading" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 3v12" />
          <path d="m7 8 5-5 5 5" />
          <path d="M5 21h14" />
        </svg>
        <svg v-else class="spinning" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M21 12a9 9 0 0 0-15.55-6.2L3 8" />
          <path d="M3 3v5h5" />
        </svg>
        <span>{{ localBookUploading ? '导入中' : '导入' }}</span>
      </button>

      <button
        type="button"
        class="rail-item"
        title="导出书籍"
        @click="showExportModal = true"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
          <path d="M7 10l5 5 5-5" />
          <path d="M12 15V3" />
        </svg>
        <span>导出</span>
      </button>

      <button
        type="button"
        class="rail-item"
        title="缓存管理"
        @click="showCacheManager = true"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <ellipse cx="12" cy="5" rx="8" ry="3" />
          <path d="M4 5v6c0 1.66 3.58 3 8 3s8-1.34 8-3V5" />
          <path d="M4 11v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6" />
        </svg>
        <span>缓存</span>
      </button>

      <button
        type="button"
        class="rail-item"
        title="书源管理"
        @click="openSourceManager"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M4 19.5V5a2 2 0 0 1 2-2h3v18H6a2 2 0 0 1-2-1.5Z" />
          <path d="M9 5h4v16H9" />
          <path d="m15 4 3-.8 3.8 15.4-3.8.9Z" />
        </svg>
        <span>书源</span>
      </button>

      <template v-if="shelfStore.editMode">
        <button
          type="button"
          class="rail-item"
          title="全选"
          @click="shelfStore.selectAll()"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M9 11l3 3L22 4" />
            <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
          </svg>
          <span>全选</span>
        </button>
        <button
          type="button"
          class="rail-item"
          title="取消全选"
          @click="shelfStore.clearSelection()"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="3" y="3" width="18" height="18" rx="2" />
            <path d="M8 12h8" />
          </svg>
          <span>取消选择</span>
        </button>
      </template>

      <button
        type="button"
        class="rail-item"
        :class="{ active: shelfStore.editMode }"
        :title="shelfStore.editMode ? '完成' : '编辑'"
        @click="toggleEditMode"
      >
        <svg v-if="shelfStore.editMode" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M20 6 9 17l-5-5" />
        </svg>
        <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 20h9" />
          <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
        </svg>
        <span>{{ shelfStore.editMode ? '完成' : '编辑' }}</span>
      </button>
    </aside>

    <input
      ref="localBookFileInputRef"
      type="file"
      accept=".txt,.epub,.pdf,text/plain,application/epub+zip,application/pdf"
      class="hidden-input"
      @change="handleLocalBookFileChange"
    />

    <!-- ========== 书架视图 ========== -->
    <div v-if="view === 'shelf'" class="shelf-content" :class="{ 'search-mode': shelfStore.isSearchMode }">
      <!-- Search Mode -->
      <SearchResults
        v-if="shelfStore.isSearchMode"
        @back="shelfStore.clearSearch()"
      />

      <!-- Normal Bookshelf View -->
      <template v-else>
        <!-- Shelf Header：标题 + 常驻搜索框 + 分组管理 -->
        <div class="shelf-header">
          <h1 class="shelf-title">
            小说
            <span class="book-count">({{ shelfStore.filteredBooks.length }})</span>
          </h1>
          <div class="shelf-header-actions">
            <form class="shelf-search-bar" role="search" @submit.prevent="submitShelfSearch">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                <circle cx="11" cy="11" r="8" />
                <path d="m21 21-4.3-4.3" />
              </svg>
              <input
                v-model="searchValue"
                type="search"
                placeholder="搜索书名或作者"
                aria-label="搜索书籍关键字"
                @keyup.esc="searchValue = ''"
              />
              <button v-if="searchValue.trim()" class="shelf-search-clear" type="button" @click="searchValue = ''" title="清空">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                  <circle cx="12" cy="12" r="9" />
                  <path d="m8 8 8 8M16 8l-8 8" />
                </svg>
              </button>
            </form>
            <button class="header-icon-btn" type="button" title="分组管理" @click="showGroupManager = true">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M4 7h6l2 2h8v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2Z" />
                <path d="M8 13h8" />
              </svg>
            </button>
          </div>
        </div>

        <!-- Group Tabs -->
        <div class="group-tabs">
          <div class="tabs-scroll">
            <button
              v-for="group in shelfStore.displayGroups"
              :key="group.groupId"
              class="tab-item"
              :class="{ active: shelfStore.activeGroupId === group.groupId }"
              @click="shelfStore.activeGroupId = group.groupId"
            >
              {{ group.groupName }}
            </button>
          </div>
          <div ref="sortMenuRef" class="sort-trigger-wrap">
            <button
              type="button"
              class="sort-trigger"
              :class="{ open: sortMenuOpen }"
              :aria-expanded="sortMenuOpen"
              aria-haspopup="listbox"
              title="书架排序方式"
              @click="toggleSortMenu"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                <path d="M3 6h12M3 12h9M3 18h6" />
                <path d="m17 8 3-3 3 3M20 5v14" />
              </svg>
              <span>{{ bookSortLabel(shelfStore.effectiveSortMode) }}</span>
              <svg class="sort-caret" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" aria-hidden="true">
                <path d="m6 9 6 6 6-6" />
              </svg>
            </button>
            <Transition name="sort-pop">
              <div v-if="sortMenuOpen" class="sort-popover" role="listbox" aria-label="书架排序方式">
                <button
                  v-for="option in BOOK_SORT_OPTIONS"
                  :key="option.value"
                  type="button"
                  class="sort-option"
                  :class="{ active: option.value === shelfStore.effectiveSortMode }"
                  role="option"
                  :aria-selected="option.value === shelfStore.effectiveSortMode"
                  @click="chooseSort(option.value)"
                >
                  <span class="sort-option-dot" aria-hidden="true">
                    <svg v-if="option.value === shelfStore.effectiveSortMode" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
                      <path d="M20 6 9 17l-5-5" />
                    </svg>
                  </span>
                  <span class="sort-option-label">{{ option.label }}</span>
                </button>
              </div>
            </Transition>
          </div>
        </div>

        <!-- Book Grid -->
        <div class="shelf-grid-wrapper">
          <BookGrid
            :books="shelfStore.sortedBooks"
            :edit-mode="shelfStore.editMode"
            :selected-urls="shelfStore.selectedBookUrls"
            :loading="shelfStore.loading"
            :sortable="sortable"
            empty-text="这里还没有小说，搜索添加新书吧"
            @click="handleBookClick"
            @info="handleBookInfo"
            @delete="handleDeleteBook"
            @select="shelfStore.toggleSelection($event.bookUrl)"
            @reorder="handleReorderBooks"
            @contextmenu="handleBookContextMenu"
          />
        </div>

        <!-- Batch Toolbar -->
        <Transition name="slide-up">
          <div v-if="shelfStore.editMode && shelfStore.selectedBookUrls.size > 0" class="batch-toolbar">
            <div class="batch-info">
              已选中 <span>{{ shelfStore.selectedBookUrls.size }}</span> 本书
            </div>
            <div class="batch-actions">
              <button class="batch-btn" @click="handleBulkMove">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 5v14M5 12h14" /></svg>
                移动分组
              </button>
              <button class="batch-btn danger" @click="handleBulkDelete">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 6h18M19 6v14a3 3 0 0 1-3-3H7c-1 0-2-1-2-2V6M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" /></svg>
                批量删除
              </button>
            </div>
          </div>
        </Transition>
      </template>
    </div>

    <!-- ========== 历史视图 ========== -->
    <div v-else class="shelf-content history-content">
      <div class="history-header">
        <h1 class="shelf-title">
          阅读历史
          <span class="book-count">({{ filteredRecentBooks.length }})</span>
        </h1>
        <button
          class="recent-clear-btn"
          :disabled="!shelfStore.recentBooks.length"
          @click="handleClearRecent"
        >
          一键清空
        </button>
      </div>

      <div class="history-stats">
        <div class="stat-card">
          <strong>{{ formatDuration(historyStats.todaySeconds) }}</strong>
          <small>今日阅读</small>
        </div>
        <div class="stat-card">
          <strong>{{ formatDuration(historyStats.monthSeconds) }}</strong>
          <small>近 30 天</small>
        </div>
        <div class="stat-card">
          <strong>{{ formatDuration(historyStats.totalSeconds) }}</strong>
          <small>累计阅读</small>
        </div>
        <div class="stat-card">
          <strong>{{ formatNumber(historyStats.totalCharacters) }}</strong>
          <small>累计字数</small>
        </div>
      </div>

      <div class="history-search-row">
        <input
          v-model.trim="historySearch"
          class="history-search-input"
          placeholder="搜索最近阅读"
        />
      </div>

      <div class="shelf-grid-wrapper">
        <BookGrid
          :books="filteredRecentBooks"
          :loading="shelfStore.loading"
          empty-text="暂无最近阅读"
          :show-delete-action="true"
          @click="handleHistoryBookClick"
          @info="handleHistoryBookInfo"
          @delete="handleRecentDelete"
          @contextmenu="handleHistoryBookContextMenu"
        />
      </div>
    </div>

    <!-- Book Detail Modal -->
    <BookDetailModal
      v-model="showDetail"
      :book="selectedBook"
      :start-in-edit="openDetailInEdit"
    />

    <!-- Group Select Modal -->
    <GroupSelectModal
      v-model="showGroupSelect"
      :books="groupEditTargets"
      :book-count="groupEditTargets.length"
      :submitting="groupSaving"
      @confirm="handleGroupConfirm"
    />
    <GroupManagerModal v-model="showGroupManager" />

    <CacheLibraryModal v-model="showCacheManager" />
    <ExportBookModal v-model="showExportModal" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onBeforeUnmount, onMounted, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useBookshelfStore } from '../stores/bookshelf'
import { useReaderStore } from '../stores/reader'
import { useAppStore } from '../stores/app'
import { uploadEpubBook, uploadPdfBook, uploadTxtBook } from '../api/bookshelf'
import { getReadingStatsDaily, getReadingStatsSummary } from '../api/readingStats'
import BookGrid from '../components/BookGrid.vue'
import BookDetailModal from '../components/BookDetailModal.vue'
import GroupSelectModal from '../components/bookshelf/GroupSelectModal.vue'
import GroupManagerModal from '../components/bookshelf/GroupManagerModal.vue'
import SearchResults from '../components/SearchResults.vue'
import CacheLibraryModal from '../components/CacheLibraryModal.vue'
import ExportBookModal from '../components/ExportBookModal.vue'
import { showContextMenu } from '../composables/useContextMenu'
import { BOOK_SORT_KEYS, BOOK_SORT_OPTIONS, bookSortLabel } from '../utils/bookshelfSort'
import { recordRssReadFromBook } from '../utils/rssReadHistory'
import type { Book, SearchBook } from '../types'

// ─── 视图切换: 书架 / 历史 ───
type HomeView = 'shelf' | 'history'
const view = ref<HomeView>('shelf')

// 支持深链接 ?view=history
const router = useRouter()
watch(
  () => router.currentRoute.value.query.view,
  (q) => { if (q === 'history') view.value = 'history' },
  { immediate: true },
)

// ─── 基础 store / app ───
const shelfStore = useBookshelfStore()
const readerStore = useReaderStore()
const appStore = useAppStore()

const showDetail = ref(false)
const showGroupSelect = ref(false)
const groupTargetBookUrl = ref('')
const groupSaving = ref(false)
const showGroupManager = ref(false)
const showCacheManager = ref(false)
const showExportModal = ref(false)
const selectedBook = ref<Book | SearchBook | null>(null)
const openDetailInEdit = ref(false)
watch(showDetail, (visible) => {
  if (!visible) openDetailInEdit.value = false
})
const openingBookUrl = ref('')
const localBookFileInputRef = ref<HTMLInputElement | null>(null)
const localBookUploading = ref(false)

// ─── 搜索框(常驻 header) ───
const searchValue = ref('')

function submitShelfSearch() {
  const value = searchValue.value.trim()
  if (!value) return
  shelfStore.startSearch(value, { scope: 'all', sourceUrl: '' })
}

function openSourceManager() {
  appStore.showSourceManager = true
}

// ─── 分组编辑器的目标书 ───
const groupEditTargets = computed(() => {
  const target = groupTargetBookUrl.value
  if (target) {
    const book = shelfStore.books.find((b) => b.bookUrl === target)
    return book ? [book] : []
  }
  return shelfStore.books.filter((b) => shelfStore.selectedBookUrls.has(b.bookUrl))
})

onMounted(async () => {
  let skipOfflineRestore = false
  try {
    skipOfflineRestore = sessionStorage.getItem('reader-skip-offline-restore') === '1'
    sessionStorage.removeItem('reader-skip-offline-restore')
  } catch {
    // Ignore unavailable session storage.
  }
  await Promise.all([
    shelfStore.fetchBooks().catch(() => undefined),
    shelfStore.fetchGroups().catch(() => undefined),
  ])
  if (!skipOfflineRestore && !appStore.isOnline) {
    const restored = await readerStore.restorePersistedSession()
    if (restored) {
      appStore.showToast('已恢复最近阅读的离线章节', 'success')
      router.replace('/reader')
    }
  }
})

// ─── 历史视图相关 ───
const historySearch = ref('')
const historyStats = ref({
  todaySeconds: 0,
  monthSeconds: 0,
  totalSeconds: 0,
  totalCharacters: 0,
})

function dateStr(offsetDays: number) {
  const date = new Date()
  date.setDate(date.getDate() - offsetDays)
  return date.toISOString().slice(0, 10)
}

async function loadHistoryStats() {
  const end = dateStr(0)
  const start = dateStr(29)
  const [daily, summary] = await Promise.all([
    getReadingStatsDaily(start, end),
    getReadingStatsSummary(),
  ])
  historyStats.value = {
    todaySeconds: daily.find((item) => item.date === end)?.seconds ?? 0,
    monthSeconds: daily.reduce((total, item) => total + item.seconds, 0),
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
  const keyword = historySearch.value.trim().toLowerCase()
  if (!keyword) return list
  return list.filter((book) =>
    [
      book.name, book.author, book.intro, book.originName,
      book.origin, book.latestChapterTitle, book.durChapterTitle,
    ]
      .filter(Boolean)
      .some((value) => String(value).toLowerCase().includes(keyword)),
  )
})

// 切到历史视图时加载数据
watch(view, async (v) => {
  if (v !== 'history') return
  await shelfStore.refreshRecentBooks().catch(() => undefined)
  // 旧版本的 RSS 记录只存在「最近阅读」里，迁移到 RSS 独立历史，避免统计丢失。
  for (const book of shelfStore.recentBooks) recordRssReadFromBook(book)
  await loadHistoryStats().catch(() => undefined)
}, { immediate: view.value === 'history' })

async function handleHistoryBookClick(book: Book | SearchBook) {
  const b = book as Book
  if (b.recentKind === 'rss' && b.rssSourceUrl && (b.rssLink || b.bookUrl)) {
    await router.push({
      name: 'rss-article',
      query: {
        source: b.rssSourceUrl,
        link: b.rssLink || b.bookUrl,
        title: b.name || '',
        pubDate: b.rssPubDate || '',
        origin: b.author || '',
      },
    })
    return
  }
  if (!b.origin || !b.bookUrl) return
  if (openingBookUrl.value === b.bookUrl) return

  openingBookUrl.value = b.bookUrl
  try {
    await shelfStore.moveBookToFront(b.bookUrl).catch(() => undefined)
    const loadBookTask = readerStore.loadBook(b)
    await router.push('/reader')
    await loadBookTask
  } finally {
    openingBookUrl.value = ''
  }
}

function handleHistoryBookInfo(book: Book | SearchBook) {
  const b = book as Book
  if (b.recentKind === 'rss') {
    void handleHistoryBookClick(book)
    return
  }
  selectedBook.value = book
  showDetail.value = true
}

async function handleRecentDelete(book: Book | SearchBook) {
  await shelfStore.removeRecentBook(book as Book).catch(() => undefined)
}

function handleHistoryBookContextMenu({ book, event }: { book: Book | SearchBook; event: MouseEvent }) {
  const b = book as Book
  const menuItems = [
    { label: b.recentKind === 'rss' ? '打开文章' : '开始阅读', action: () => handleHistoryBookClick(book) },
    { label: '查看详情', action: () => handleHistoryBookInfo(book) },
    { divider: true },
    { label: '从最近移除', danger: true, action: () => handleRecentDelete(book) },
  ]
  showContextMenu(event, menuItems, book)
}

async function handleClearRecent() {
  await shelfStore.clearAllRecentBooks().catch(() => undefined)
}

// ─── 书架书籍操作 ───
function triggerLocalBookUpload() {
  if (localBookUploading.value) return
  localBookFileInputRef.value?.click()
}

async function handleLocalBookFileChange(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return

  const lowerName = file.name.toLowerCase()
  if (!lowerName.endsWith('.txt') && !lowerName.endsWith('.epub') && !lowerName.endsWith('.pdf')) {
    appStore.showToast('只支持导入 .txt、.epub 或 .pdf 文件', 'warning')
    return
  }

  localBookUploading.value = true
  try {
    const book = lowerName.endsWith('.epub')
      ? await uploadEpubBook(file)
      : lowerName.endsWith('.pdf')
        ? await uploadPdfBook(file)
        : await uploadTxtBook(file)
    await shelfStore.fetchBooks()
    appStore.showToast(`已导入《${book.name}》`, 'success')
    await handleBookClick(book)
  } catch (e: unknown) {
    appStore.showToast((e as Error).message || '本地书导入失败', 'error')
  } finally {
    localBookUploading.value = false
  }
}

async function handleBookClick(book: Book | SearchBook) {
  const b = book as Book
  if (openingBookUrl.value === b.bookUrl) return

  openingBookUrl.value = b.bookUrl
  try {
    await shelfStore.moveBookToFront(b.bookUrl).catch(() => undefined)
    const loadBookTask = readerStore.loadBook(b)
    await router.push('/reader')
    await loadBookTask
  } finally {
    openingBookUrl.value = ''
  }
}

function handleBookInfo(book: Book | SearchBook) {
  openDetail(book, false)
}

function openDetail(book: Book | SearchBook, editNow: boolean) {
  selectedBook.value = book
  openDetailInEdit.value = editNow
  showDetail.value = true
}

async function handleDeleteBook(book: Book | SearchBook) {
  const b = book as Book
  const ok = await appStore.confirmDialog(`确定从书架删除 "${b.name}"？`, { title: '删除书籍', danger: true })
  if (!ok) return
  try {
    await shelfStore.removeBook(b)
    appStore.showToast(`已删除 "${b.name}"`, 'success')
  } catch (e: unknown) {
    appStore.showToast((e as Error).message, 'error')
  }
}

function handleBookContextMenu({ book, event }: { book: Book | SearchBook; event: MouseEvent }) {
  if (shelfStore.editMode) return
  const menuItems = [
    { label: '开始阅读', action: () => handleBookClick(book) },
    { label: '查看详情', action: () => handleBookInfo(book) },
    { label: '编辑信息', action: () => openDetail(book, true) },
    { divider: true },
    { label: '调整分组', action: () => openGroupSelect(book.bookUrl) },
    { label: '缓存管理', action: () => { showCacheManager.value = true } },
    { divider: true },
    { label: '删除', danger: true, action: () => handleDeleteBook(book) },
  ]
  showContextMenu(event, menuItems, book)
}

function toggleEditMode() {
  shelfStore.editMode = !shelfStore.editMode
  if (!shelfStore.editMode) shelfStore.clearSelection()
}

async function handleBulkDelete() {
  const count = shelfStore.selectedBookUrls.size
  const ok = await appStore.confirmDialog(`确定删除选中的 ${count} 本书？`, { title: '批量删除', danger: true })
  if (!ok) return
  try {
    await shelfStore.bulkDelete()
    appStore.showToast(`成功删除 ${count} 本书`, 'success')
  } catch (e: any) {
    appStore.showToast(e.message, 'error')
  }
}

async function handleBulkMove() {
  openGroupSelect('')
}

function openGroupSelect(bookUrl: string) {
  groupTargetBookUrl.value = bookUrl
  showGroupSelect.value = true
}

async function handleGroupConfirm(mask: number) {
  const target = groupTargetBookUrl.value
  groupSaving.value = true
  try {
    if (target) {
      await shelfStore.setBooksGroupMask([target], mask)
      appStore.showToast('分组已更新', 'success')
    } else {
      const urls = [...shelfStore.selectedBookUrls]
      await shelfStore.setBooksGroupMask(urls, mask)
      appStore.showToast(`已调整 ${urls.length} 本书的分组`, 'success')
      shelfStore.clearSelection()
    }
    showGroupSelect.value = false
  } catch (e: any) {
    appStore.showToast(e.message || '分组更新失败', 'error')
  } finally {
    groupSaving.value = false
  }
}
watch(showGroupSelect, (visible) => {
  if (!visible) groupTargetBookUrl.value = ''
})

async function handleReorderBooks(payload: { draggedUrl: string; targetUrl: string }) {
  try {
    await shelfStore.reorderBooks(payload.draggedUrl, payload.targetUrl)
  } catch (e: any) {
    appStore.showToast(e.message || '排序失败', 'error')
  }
}

const sortable = computed(() => (
  shelfStore.effectiveSortMode === BOOK_SORT_KEYS.manual
  && !shelfStore.editMode
  && !shelfStore.loading
  && !shelfStore.sorting
  && !shelfStore.isSearchMode
))

// ─── 排序下拉 ───
const sortMenuOpen = ref(false)
const sortMenuRef = ref<HTMLElement | null>(null)

async function applySort(value: number) {
  if (shelfStore.activeGroupId > 0) {
    await shelfStore.setActiveGroupSort(value)
  } else {
    shelfStore.setSortMode(value)
  }
}

async function chooseSort(value: number) {
  sortMenuOpen.value = false
  try {
    await applySort(value)
  } catch (e: any) {
    appStore.showToast(e.message || '排序设置失败', 'error')
  }
}

function toggleSortMenu() {
  sortMenuOpen.value = !sortMenuOpen.value
}

function onSortMenuOutsideClick(event: MouseEvent) {
  const target = event.target as Node | null
  if (target && sortMenuRef.value?.contains(target)) return
  sortMenuOpen.value = false
}

function onSortMenuKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') sortMenuOpen.value = false
}

watch(sortMenuOpen, (open) => {
  if (open) {
    window.addEventListener('click', onSortMenuOutsideClick, true)
    window.addEventListener('keydown', onSortMenuKeydown)
  } else {
    window.removeEventListener('click', onSortMenuOutsideClick, true)
    window.removeEventListener('keydown', onSortMenuKeydown)
  }
})

onBeforeUnmount(() => {
  window.removeEventListener('click', onSortMenuOutsideClick, true)
  window.removeEventListener('keydown', onSortMenuKeydown)
})
</script>

<style scoped>
.home-view {
  height: 100%;
  min-height: 0;
  overflow: hidden;
  display: flex;
  padding: 0 var(--space-6) var(--space-4) 0;
}

/* 左侧功能栏 */
.shelf-rail {
  flex-shrink: 0;
  width: 56px;
  padding: var(--space-3) 4px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  border-right: 1px solid var(--color-divider);
  overflow-y: auto;
  scrollbar-width: none;
}
.shelf-rail::-webkit-scrollbar { display: none; }

.rail-item {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  gap: 4px;
  padding: 8px 2px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 11px;
  transition: all var(--duration-fast) var(--ease-out);
}
.rail-item svg { width: 20px; height: 20px; flex-shrink: 0; }
.rail-item:hover { background: var(--color-bg-hover); color: var(--color-text); }
.rail-item.active { background: var(--color-primary-bg); color: var(--color-primary); font-weight: 600; }
.rail-item:disabled { opacity: 0.55; cursor: wait; }
.rail-item .spinning { animation: shelf-spin 0.9s linear infinite; }

.shelf-content {
  flex: 1;
  min-width: 0;
  height: 100%;
  min-height: 0;
  padding-left: var(--space-4);
  display: flex;
  flex-direction: column;
}
.shelf-content.search-mode { padding-left: 0; }

.hidden-input { display: none; }

/* Header: 标题 + 搜索 + 分组按钮 */
.shelf-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  padding: var(--space-6) 0 var(--space-3);
  flex-shrink: 0;
}

.shelf-title { font-size: var(--text-2xl); font-weight: 700; letter-spacing: 0; }
.book-count { font-size: var(--text-base); font-weight: 400; color: var(--color-text-tertiary); }

.shelf-header-actions {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex-shrink: 0;
}

/* 常驻搜索框 */
.shelf-search-bar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 240px;
  padding: 6px 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
}
.shelf-search-bar svg { width: 16px; height: 16px; flex-shrink: 0; color: var(--color-text-tertiary); }
.shelf-search-bar input {
  flex: 1; min-width: 0; border: none; outline: none; background: none;
  font-size: var(--text-sm); color: var(--color-text);
}
.shelf-search-bar input::placeholder { color: var(--color-text-tertiary); }
.shelf-search-clear {
  padding: 0; border: none; background: none; color: var(--color-text-tertiary);
  cursor: pointer; display: flex; align-items: center;
}
.shelf-search-clear:hover { color: var(--color-text-secondary); }
.shelf-search-clear svg { width: 14px; height: 14px; }

.header-icon-btn {
  width: 34px; height: 34px; display: flex; align-items: center; justify-content: center;
  border-radius: var(--radius-md); border: 1px solid transparent; background: transparent;
  color: var(--color-text-tertiary); cursor: pointer; transition: all var(--duration-fast);
}
.header-icon-btn:hover { background: var(--color-bg-hover); color: var(--color-text); border-color: var(--color-border); }
.header-icon-btn svg { width: 18px; height: 18px; }

@keyframes shelf-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

/* Group Tabs */
.group-tabs {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  border-bottom: 1px solid var(--color-border-light);
  margin-bottom: var(--space-2);
  flex-shrink: 0;
}
.tabs-scroll { flex: 1; min-width: 0; display: flex; gap: 0; overflow-x: auto; scrollbar-width: none; -ms-overflow-style: none; }
.tabs-scroll::-webkit-scrollbar { display: none; }
.tab-item {
  padding: var(--space-3) var(--space-5);
  font-size: var(--text-sm); font-weight: 500; color: var(--color-text-tertiary);
  white-space: nowrap; position: relative; transition: color var(--duration-fast);
  border-bottom: 2px solid transparent; margin-bottom: -1px;
}
.tab-item:hover { color: var(--color-text-secondary); }
.tab-item.active { color: var(--color-primary); border-bottom-color: var(--color-primary); }

.sort-trigger-wrap { flex-shrink: 0; position: relative; }
.sort-trigger {
  display: inline-flex; align-items: center; gap: 6px;
  padding: 5px 8px 5px 10px; border: 1px solid transparent; border-radius: var(--radius-md);
  background: transparent; color: var(--color-text-tertiary);
  font-size: var(--text-xs); font-weight: 500; line-height: 1.4; white-space: nowrap;
  transition: color var(--duration-fast), background var(--duration-fast), border-color var(--duration-fast);
}
.sort-trigger svg { width: 14px; height: 14px; flex-shrink: 0; }
.sort-trigger .sort-caret { width: 12px; height: 12px; opacity: 0.65; transition: transform var(--duration-fast); }
.sort-trigger:hover { color: var(--color-text-secondary); background: var(--color-bg-hover); }
.sort-trigger.open { color: var(--color-text); background: var(--color-bg-hover); border-color: var(--color-border); }
.sort-trigger.open .sort-caret { transform: rotate(180deg); }

.sort-popover {
  position: absolute; top: calc(100% + 6px); right: 0; z-index: var(--z-dropdown, 60);
  min-width: 156px; padding: 5px;
  border: 1px solid color-mix(in srgb, var(--color-border) 80%, transparent); border-radius: var(--radius-lg);
  background: color-mix(in srgb, var(--color-bg-elevated) 94%, transparent);
  box-shadow: var(--shadow-xl); backdrop-filter: blur(20px) saturate(140%); -webkit-backdrop-filter: blur(20px) saturate(140%);
}
.sort-option {
  display: flex; align-items: center; gap: 8px; width: 100%; min-height: 32px; padding: 0 8px;
  border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--color-text-secondary);
  font-size: var(--text-xs); text-align: left; transition: background var(--duration-fast), color var(--duration-fast);
}
.sort-option:hover { background: var(--color-bg-hover); color: var(--color-text); }
.sort-option.active { color: var(--color-primary); font-weight: 600; }
.sort-option-dot { display: inline-flex; align-items: center; justify-content: center; width: 14px; height: 14px; flex-shrink: 0; }
.sort-option-dot svg { width: 13px; height: 13px; }
.sort-option-label { flex: 1; }

.sort-pop-enter-active, .sort-pop-leave-active { transition: opacity var(--duration-fast), transform var(--duration-fast); }
.sort-pop-enter-from, .sort-pop-leave-to { opacity: 0; transform: translateY(-4px) scale(0.98); }

/* Book Grid */
.shelf-grid-wrapper {
  flex: 1; min-height: 0; overflow: auto; padding-bottom: calc(76px + var(--space-6));
  scrollbar-width: none; -ms-overflow-style: none;
}
.shelf-grid-wrapper::-webkit-scrollbar { display: none; }

.batch-toolbar {
  position: fixed; bottom: calc(76px + var(--space-4)); left: 50%; transform: translateX(-50%);
  background: var(--color-bg-elevated); border: 1px solid var(--color-border); box-shadow: var(--shadow-xl);
  border-radius: var(--radius-xl); padding: var(--space-3) var(--space-6);
  display: flex; align-items: center; gap: var(--space-8);
  z-index: calc(var(--z-sticky) + 5); backdrop-filter: blur(12px);
}
.batch-info { font-size: var(--text-sm); color: var(--color-text-secondary); }
.batch-info span { font-weight: 700; color: var(--color-primary); margin: 0 4px; }
.batch-actions { display: flex; gap: var(--space-2); }
.batch-btn {
  display: flex; align-items: center; gap: var(--space-2);
  padding: var(--space-2) var(--space-4); border-radius: var(--radius-lg);
  font-size: var(--text-sm); font-weight: 600;
  background: var(--color-bg-hover); color: var(--color-text-secondary); transition: all var(--duration-fast);
}
.batch-btn:hover { background: var(--color-bg-sunken); color: var(--color-text); }
.batch-btn.danger { color: var(--color-danger); }
.batch-btn svg { width: 16px; height: 16px; }

.slide-up-enter-active, .slide-up-leave-active { transition: all var(--duration-normal) var(--ease-out); }
.slide-up-enter-from, .slide-up-leave-to { opacity: 0; transform: translate(-50%, 20px); }

/* ========== 历史视图 ========== */
.history-content { padding-bottom: var(--space-6); }

.history-header {
  padding: var(--space-6) 0 var(--space-3);
  display: flex; align-items: center; justify-content: space-between; gap: var(--space-4);
  flex-shrink: 0;
}

.recent-clear-btn {
  padding: 10px 16px; border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light); background: var(--color-bg-elevated);
  color: var(--color-text-secondary); font-size: var(--text-sm); transition: all var(--duration-fast);
}
.recent-clear-btn:hover:not(:disabled) { border-color: rgba(225, 76, 76, 0.22); color: var(--color-danger); }
.recent-clear-btn:disabled { opacity: 0.45; cursor: not-allowed; }

.history-stats {
  display: flex; flex-wrap: wrap; gap: var(--space-3);
  padding-bottom: var(--space-3); flex-shrink: 0;
}
.stat-card {
  min-width: 120px; padding: 10px 14px; border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-light); background: var(--color-bg-elevated);
  display: flex; flex-direction: column; gap: 2px;
}
.stat-card strong { color: var(--color-text); font-size: 17px; font-weight: 600; }
.stat-card small { color: var(--color-text-tertiary); font-size: 11px; }

.history-search-row { padding-bottom: var(--space-3); flex-shrink: 0; }
.history-search-input {
  width: 100%; border: 1px solid var(--color-border-light); background: var(--color-bg-elevated);
  border-radius: var(--radius-lg); padding: 12px 14px; font-size: var(--text-sm); color: var(--color-text);
}
.history-search-input::placeholder { color: var(--color-text-tertiary); }

/* 历史视图下 grid 不需要额外底部 padding(没有固定播放器) */
.history-content .shelf-grid-wrapper { padding-bottom: var(--space-6); }

/* ========== 响应式 ========== */
@media (max-width: 640px) {
  .home-view { padding: 0 var(--space-3) var(--space-3) 0; }
  .shelf-rail { width: 48px; padding: var(--space-2) 2px; }
  .rail-item { font-size: 10px; padding: 7px 1px; }
  .rail-item svg { width: 18px; height: 18px; }
  .shelf-content { padding-left: var(--space-3); }

  .shelf-header {
    flex-wrap: wrap; align-items: center; gap: var(--space-2);
    padding: var(--space-5) 0 var(--space-3);
  }
  .shelf-title { flex: 0 0 auto; font-size: var(--text-2xl); }
  .shelf-header-actions { flex: 1 1 100%; order: 3; }
  .shelf-search-bar { flex: 1; width: auto; }

  .history-header { flex-direction: column; align-items: flex-start; }
  .recent-clear-btn { align-self: flex-end; }

  .batch-toolbar {
    width: calc(100% - var(--space-8)); bottom: calc(76px + var(--space-3));
    gap: var(--space-4); justify-content: space-between;
  }
  .batch-info { display: none; }
}
</style>
