import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { formatRelativeTime } from './time'

describe('formatRelativeTime', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-10-23T12:00:00Z'))
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('returns empty for a missing value', () => {
    expect(formatRelativeTime()).toBe('')
    expect(formatRelativeTime('')).toBe('')
  })

  it('returns the original text when the date cannot be parsed', () => {
    expect(formatRelativeTime('未知时间')).toBe('未知时间')
  })

  it('treats future and just-past timestamps as 刚刚', () => {
    expect(formatRelativeTime('2026-10-23T13:00:00Z')).toBe('刚刚')
    expect(formatRelativeTime('2026-10-23T11:59:30Z')).toBe('刚刚')
  })

  it('formats minutes, hours and days', () => {
    expect(formatRelativeTime('2026-10-23T11:55:00Z')).toBe('5 分钟前')
    expect(formatRelativeTime('2026-10-23T09:00:00Z')).toBe('3 小时前')
    expect(formatRelativeTime('2026-10-21T12:00:00Z')).toBe('2 天前')
  })

  it('formats months and years', () => {
    expect(formatRelativeTime('2026-07-25T12:00:00Z')).toBe('3 个月前')
    expect(formatRelativeTime('2024-10-23T12:00:00Z')).toBe('2 年前')
  })
})
