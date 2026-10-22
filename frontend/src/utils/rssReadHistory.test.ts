import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  clearRssReadHistory,
  loadRssReadHistory,
  recordRssRead,
  removeRssRead,
  summarizeRssRead,
} from './rssReadHistory'

function installLocalStorage() {
  const values = new Map<string, string>()
  vi.stubGlobal('localStorage', {
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => values.set(key, value)),
    removeItem: vi.fn((key: string) => values.delete(key)),
  })
  return values
}

const DAY = 24 * 60 * 60 * 1000
const NOW = new Date(2026, 9, 22, 12, 0, 0).getTime()

describe('rss read history', () => {
  beforeEach(() => {
    vi.unstubAllGlobals()
    installLocalStorage()
  })

  it('dedupes by article and keeps the newest entry on top', () => {
    recordRssRead({ title: '旧标题', sourceUrl: 'https://a.com/rss', link: 'https://a.com/1', readAt: NOW - 1000 })
    recordRssRead({ title: '新标题', sourceUrl: 'https://a.com/rss', link: 'https://a.com/1', readAt: NOW })
    recordRssRead({ title: '另一篇', sourceUrl: 'https://b.com/rss', link: 'https://b.com/2', readAt: NOW - 500 })

    const entries = loadRssReadHistory()
    expect(entries).toHaveLength(2)
    expect(entries[0].title).toBe('新标题')
    expect(entries[1].title).toBe('另一篇')
  })

  it('does not overwrite a newer timestamp when migrating an old entry', () => {
    recordRssRead({ title: '文章', sourceUrl: 'https://a.com/rss', link: 'https://a.com/1', readAt: NOW })
    recordRssRead({ title: '文章', sourceUrl: 'https://a.com/rss', link: 'https://a.com/1', readAt: NOW - 5 * DAY })

    expect(loadRssReadHistory()[0].readAt).toBe(NOW)
  })

  it('summarizes today, last 30 days and total', () => {
    recordRssRead({ title: '今天', sourceUrl: 'https://a.com/rss', link: 'https://a.com/1', readAt: NOW })
    recordRssRead({ title: '五天前', sourceUrl: 'https://a.com/rss', link: 'https://a.com/2', readAt: NOW - 5 * DAY })
    recordRssRead({ title: '四十天前', sourceUrl: 'https://a.com/rss', link: 'https://a.com/3', readAt: NOW - 40 * DAY })

    expect(summarizeRssRead(loadRssReadHistory(), NOW)).toEqual({
      today: 1,
      month: 2,
      total: 3,
    })
  })

  it('removes a single entry and clears all', () => {
    recordRssRead({ title: 'A', sourceUrl: 'https://a.com/rss', link: 'https://a.com/1', readAt: NOW })
    recordRssRead({ title: 'B', sourceUrl: 'https://a.com/rss', link: 'https://a.com/2', readAt: NOW })

    removeRssRead('https://a.com/rss::https://a.com/1')
    expect(loadRssReadHistory().map((entry) => entry.title)).toEqual(['B'])

    clearRssReadHistory()
    expect(loadRssReadHistory()).toEqual([])
  })

  it('ignores records without source or link', () => {
    recordRssRead({ title: '无来源', sourceUrl: '', link: 'https://a.com/1', readAt: NOW })
    recordRssRead({ title: '无链接', sourceUrl: 'https://a.com/rss', link: '', readAt: NOW })

    expect(loadRssReadHistory()).toEqual([])
  })
})
