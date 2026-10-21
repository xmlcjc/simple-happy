import { describe, expect, it } from 'vitest'
import { pickAlternative } from './useMusicPlayer'
import type { MusicItem } from '../api/music'

function item(title: string, id = title): MusicItem {
  return { platform: 'demo', id, title, artist: '歌手' }
}

describe('pickAlternative', () => {
  it('returns null when the alternative source found nothing', () => {
    expect(pickAlternative([], item('晴天'))).toBeNull()
  })

  it('prefers the candidate whose normalized title matches', () => {
    const candidates = [item('雨天', 'a'), item('晴天 (Live)', 'b'), item('晴天', 'c')]
    expect(pickAlternative(candidates, item('晴天（钢琴版）'))?.id).toBe('b')
  })

  it('falls back to the first result when no title matches', () => {
    const candidates = [item('完全不同的歌', 'a'), item('另一首', 'b')]
    expect(pickAlternative(candidates, item('晴天'))?.id).toBe('a')
  })
})
