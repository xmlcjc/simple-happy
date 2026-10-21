import { computed, ref, watch, type Ref } from 'vue'
import { useMusicStore } from '../stores/music'
import { useMusicSettingsStore } from '../stores/musicSettings'
import type { MusicItem } from '../api/music'

export interface LyricLine {
  time: number
  text: string
  translation?: string
}

const TIME_TAG = /\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]/g

/** 解析 LRC 文本；同一行可以有多个时间戳。 */
export function parseLrc(raw: string, translation = ''): LyricLine[] {
  const main = parseLines(raw)
  if (!main.length) return []
  const translated = parseLines(translation)
  if (!translated.length) return main
  const lookup = new Map<number, string>()
  for (const line of translated) lookup.set(Math.round(line.time * 1000), line.text)
  return main.map((line) => ({
    ...line,
    translation: lookup.get(Math.round(line.time * 1000)),
  }))
}

function parseLines(raw: string): LyricLine[] {
  if (!raw) return []
  const lines: LyricLine[] = []
  for (const source of raw.split(/\r?\n/)) {
    const matches = Array.from(source.matchAll(TIME_TAG))
    if (!matches.length) continue
    const text = source.replace(TIME_TAG, '').replace(/\[[^\]]*\]/g, '').trim()
    if (!text) continue
    for (const match of matches) {
      const minutes = Number(match[1] || 0)
      const seconds = Number(match[2] || 0)
      const fraction = (match[3] || '').padEnd(3, '0').slice(0, 3)
      const millis = fraction ? Number(fraction) / 1000 : 0
      lines.push({ time: minutes * 60 + seconds + millis, text })
    }
  }
  lines.sort((a, b) => a.time - b.time)
  return lines
}

/** 二分找到当前时间对应的歌词行；返回 -1 表示还没到第一行。 */
export function findLyricIndex(lines: LyricLine[], time: number): number {
  if (!lines.length) return -1
  let low = 0
  let high = lines.length - 1
  let found = -1
  while (low <= high) {
    const mid = (low + high) >> 1
    if (lines[mid].time <= time) {
      found = mid
      low = mid + 1
    } else {
      high = mid - 1
    }
  }
  return found
}

/** 跟随当前曲目加载歌词，并把播放时间映射到高亮行。 */
export function useMusicLyric(current: Ref<MusicItem | null>, time: Ref<number>) {
  const store = useMusicStore()
  const settings = useMusicSettingsStore()
  const lines = ref<LyricLine[]>([])
  const loading = ref(false)
  const error = ref('')
  let token = 0

  watch(
    () => (current.value ? `${current.value.platform}:${current.value.id}` : ''),
    async () => {
      const item = current.value
      if (!item) {
        lines.value = []
        error.value = ''
        return
      }
      if (item.platform === 'local') {
        lines.value = []
        loading.value = false
        error.value = '本地歌曲暂无歌词'
        return
      }
      const own = ++token
      loading.value = true
      error.value = ''
      try {
        const lyric = await store.lyric(item)
        if (own !== token) return
        lines.value = parseLrc(lyric.raw, lyric.translation)
        if (!lines.value.length) error.value = '该歌曲暂无歌词'
      } catch (e) {
        if (own !== token) return
        lines.value = []
        error.value = (e as Error).message || '歌词加载失败'
      } finally {
        if (own === token) loading.value = false
      }
    },
    { immediate: true },
  )

  // 偏移为正表示歌词延后：用「播放时间 - 偏移」去匹配行时间。
  const currentIndex = computed(() =>
    findLyricIndex(lines.value, time.value - settings.lyricOffsetMs / 1000),
  )

  return { lines, loading, error, currentIndex }
}
