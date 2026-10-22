import type { Book } from '../types'

/**
 * RSS 文章阅读历史。
 *
 * 与「最近阅读」（`recentBooks`）分开存放：后者只保留 100 条、RSS 记录 7 天过期，
 * 用它做统计会严重偏低。这里按文章 key 去重并长期保留，供 RSS 历史页统计篇数。
 */

const RSS_READ_HISTORY_KEY = 'reader-rss-read-history'
/** 单机场景留足容量；超出后按最近阅读时间淘汰最旧的。 */
const MAX_RSS_READ_HISTORY = 3000

export interface RssReadEntry {
  /** `${sourceUrl}::${link}` */
  key: string
  title: string
  sourceUrl: string
  link: string
  pubDate: string
  readAt: number
}

function readAll(): RssReadEntry[] {
  try {
    const raw = localStorage.getItem(RSS_READ_HISTORY_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter(
      (item): item is RssReadEntry =>
        Boolean(item?.key) && Boolean(item?.link) && typeof item?.readAt === 'number',
    )
  } catch {
    return []
  }
}

function writeAll(entries: RssReadEntry[]) {
  try {
    localStorage.setItem(
      RSS_READ_HISTORY_KEY,
      JSON.stringify(entries.slice(0, MAX_RSS_READ_HISTORY)),
    )
  } catch {
    // 隐私模式或被禁用时静默降级，不影响阅读。
  }
}

export function rssReadKey(sourceUrl: string, link: string) {
  return `${sourceUrl}::${link}`
}

export function loadRssReadHistory(): RssReadEntry[] {
  return readAll().sort((left, right) => right.readAt - left.readAt)
}

/** 记录一次文章打开；同一篇文章只更新时间并置顶。 */
export function recordRssRead(params: {
  title: string
  sourceUrl: string
  link: string
  pubDate?: string
  readAt?: number
}) {
  const sourceUrl = String(params.sourceUrl || '').trim()
  const link = String(params.link || '').trim()
  if (!sourceUrl || !link) return
  const key = rssReadKey(sourceUrl, link)
  const entry: RssReadEntry = {
    key,
    title: params.title || 'RSS 文章',
    sourceUrl,
    link,
    pubDate: params.pubDate || '',
    readAt: params.readAt ?? Date.now(),
  }
  const previous = readAll()
  // 迁移旧记录时可能带着更早的时间，不能覆盖已经记录过的新时间。
  const existing = previous.find((item) => item.key === key)
  if (existing && existing.readAt >= entry.readAt) return
  const next = previous.filter((item) => item.key !== key)
  next.unshift(entry)
  writeAll(next)
}

/** 从「最近阅读」的 RSS 条目补齐记录（early 版本没有独立历史时的迁移）。 */
export function recordRssReadFromBook(book: Book & { recentReadAt?: number }) {
  if (book.recentKind !== 'rss') return
  const sourceUrl = book.rssSourceUrl || book.origin || ''
  const link = book.rssLink || book.bookUrl || ''
  recordRssRead({
    title: book.name || 'RSS 文章',
    sourceUrl,
    link,
    pubDate: book.rssPubDate || '',
    readAt: book.recentReadAt || book.durChapterTime || Date.now(),
  })
}

export function removeRssRead(key: string) {
  writeAll(readAll().filter((item) => item.key !== key))
}

export function clearRssReadHistory() {
  writeAll([])
}

function localDateKey(timestamp: number) {
  const date = new Date(timestamp)
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${date.getFullYear()}-${month}-${day}`
}

function startOfDay(timestamp: number) {
  const date = new Date(timestamp)
  date.setHours(0, 0, 0, 0)
  return date.getTime()
}

/** 今日 / 近 30 个自然日 / 累计阅读篇数。 */
export function summarizeRssRead(entries: RssReadEntry[], now = Date.now()) {
  const todayKey = localDateKey(now)
  const monthStart = startOfDay(now - 29 * 24 * 60 * 60 * 1000)
  let today = 0
  let month = 0
  for (const entry of entries) {
    if (localDateKey(entry.readAt) === todayKey) today += 1
    if (entry.readAt >= monthStart) month += 1
  }
  return { today, month, total: entries.length }
}
