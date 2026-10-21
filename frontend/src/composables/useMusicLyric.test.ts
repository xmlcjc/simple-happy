import { describe, expect, it } from 'vitest'
import { findLyricIndex, parseLrc } from './useMusicLyric'

describe('music lyric parsing', () => {
  it('parses timestamps and sorts lines', () => {
    const lines = parseLrc('[00:12.50]第二句\n[00:01.00]第一句\n[01:02]第三句')
    expect(lines.map((line) => line.text)).toEqual(['第一句', '第二句', '第三句'])
    expect(lines[0].time).toBeCloseTo(1)
    expect(lines[1].time).toBeCloseTo(12.5)
    expect(lines[2].time).toBeCloseTo(62)
  })

  it('expands one line with multiple timestamps', () => {
    const lines = parseLrc('[00:01.00][00:11.00]副歌')
    expect(lines).toHaveLength(2)
    expect(lines[1].time).toBeCloseTo(11)
  })

  it('merges translation lines by timestamp', () => {
    const lines = parseLrc('[00:01.00]hello\n[00:05.00]world', '[00:01.00]你好\n[00:05.00]世界')
    expect(lines[0].translation).toBe('你好')
    expect(lines[1].translation).toBe('世界')
  })

  it('finds the active line with binary search', () => {
    const lines = parseLrc('[00:01.00]a\n[00:05.00]b\n[00:09.00]c')
    expect(findLyricIndex(lines, 0.5)).toBe(-1)
    expect(findLyricIndex(lines, 1)).toBe(0)
    expect(findLyricIndex(lines, 6.2)).toBe(1)
    expect(findLyricIndex(lines, 20)).toBe(2)
    expect(findLyricIndex([], 3)).toBe(-1)
  })

  it('ignores lines without timestamps or text', () => {
    expect(parseLrc('作词：某某\n[00:02.00]\n[00:03.00]有效')).toHaveLength(1)
  })
})
