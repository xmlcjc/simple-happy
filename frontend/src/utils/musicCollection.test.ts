import { describe, expect, it } from 'vitest'
import type { MusicItem } from '../api/music'
import { needsPartExpansion } from './musicCollection'

const item = (patch: Record<string, unknown>): MusicItem => ({
  platform: 'bilibili',
  id: 'BV1q4FTeBEMj',
  title: '周杰伦精选100首',
  artist: '周杰伦',
  bvid: 'BV1q4FTeBEMj',
  ...patch,
})

describe('needsPartExpansion', () => {
  it('expands long bilibili videos that may hold a whole playlist', () => {
    expect(needsPartExpansion(item({ duration: 419 * 60 }))).toBe(true)
    expect(needsPartExpansion(item({ duration: 10 * 60 }))).toBe(true)
  })

  it('plays normal single tracks directly', () => {
    expect(needsPartExpansion(item({ duration: 269 }))).toBe(false)
    expect(needsPartExpansion(item({ duration: 599 }))).toBe(false)
  })

  it('skips items without bvid', () => {
    expect(needsPartExpansion({ ...item({ duration: 419 * 60 }), bvid: '' })).toBe(false)
    expect(needsPartExpansion({ ...item({ duration: 419 * 60 }), bvid: undefined })).toBe(false)
  })

  it('skips parts that were already expanded', () => {
    expect(
      needsPartExpansion(item({ duration: 270, id: 'BV1q4FTeBEMj~p2', _p: 2, cid: 123 })),
    ).toBe(false)
    expect(
      needsPartExpansion(item({ duration: 419 * 60, id: 'BV1q4FTeBEMj~p1', _p: 1, cid: 123 })),
    ).toBe(false)
  })
})
