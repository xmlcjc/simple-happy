import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useLiveShieldStore } from './liveShield'

function installLocalStorage(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial))
  vi.stubGlobal('localStorage', {
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => values.set(key, value)),
    removeItem: vi.fn((key: string) => values.delete(key)),
  })
  return values
}

describe('live shield store', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    setActivePinia(createPinia())
  })

  it('adds, removes and persists words', () => {
    const values = installLocalStorage()
    const store = useLiveShieldStore()

    store.addWord('广告')
    store.addWord('  广告  ') // 去重 + 去空白
    store.addWord('/^(?i)mm\\d+$/')
    store.addWord('') // 空词丢弃
    expect(store.words).toEqual(['广告', '/^(?i)mm\\d+$/'])
    expect(values.get('live-danmaku-shield-words')).toBe(
      JSON.stringify(['广告', '/^(?i)mm\\d+$/']),
    )

    store.removeWord('广告')
    expect(store.words).toEqual(['/^(?i)mm\\d+$/'])

    store.clearWords()
    expect(store.words).toEqual([])
  })

  it('blocks danmaku by keyword on content or username', () => {
    installLocalStorage()
    const store = useLiveShieldStore()
    store.addWord('代练')

    expect(store.shouldBlock('需要代练的私聊')).toBe(true)
    expect(store.shouldBlock('正常弹幕', '某代练')).toBe(true)
    expect(store.shouldBlock('正常弹幕', '正常用户')).toBe(false)
  })

  it('matches regex words wrapped in slashes, case-insensitive', () => {
    installLocalStorage()
    const store = useLiveShieldStore()
    store.addWord('/v\\d{3}/')

    expect(store.shouldBlock('出售v123账号')).toBe(true)
    expect(store.shouldBlock('出售V123账号')).toBe(true)
    expect(store.shouldBlock('正常弹幕')).toBe(false)
  })

  it('falls back to plain matching for broken regex words', () => {
    installLocalStorage()
    const store = useLiveShieldStore()
    // 非法正则: 未闭合的分组。
    store.addWord('/bad(/')

    // 坏正则当普通词处理: 原样包含匹配。
    expect(store.shouldBlock('弹幕/bad(/继续')).toBe(true)
    expect(store.shouldBlock('其它弹幕')).toBe(false)
  })

  it('restores saved words and ignores garbage storage', () => {
    installLocalStorage({
      'live-danmaku-shield-words': JSON.stringify(['关注', 42, null, '  ']),
    })
    const store = useLiveShieldStore()
    expect(store.words).toEqual(['关注'])
  })
})
