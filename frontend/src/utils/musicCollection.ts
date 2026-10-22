import type { MusicItem } from '../api/music'

/**
 * 判定为「合集型长视频」的最短总时长。
 *
 * B 站把整张歌单塞进一个视频（分P）时总时长通常远超单曲；普通单曲不会触发
 * 额外的分P探测请求，避免播放前多一次网络往返。
 */
const COMPILATION_MIN_SECONDS = 10 * 60

/**
 * 播放前是否需要展开 B 站分P。
 *
 * - 只有带 `bvid` 的条目才可能是分P视频；
 * - 已经展开过的分P（`_p` / `bvid~pN`）不再重复处理；
 * - 总时长不足阈值的条目按普通单曲直接播放。
 */
export function needsPartExpansion(item: MusicItem): boolean {
  const bvid = typeof item.bvid === 'string' ? item.bvid : ''
  if (!bvid) return false
  if (item._p) return false
  if (String(item.id ?? '').includes('~p')) return false
  return Number(item.duration || 0) >= COMPILATION_MIN_SECONDS
}
