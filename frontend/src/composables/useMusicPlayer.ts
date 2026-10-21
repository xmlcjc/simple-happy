import { computed, onBeforeUnmount, ref } from 'vue'
import { useMusicStore } from '../stores/music'
import { useMusicLibraryStore } from '../stores/musicLibrary'
import { readerOrigin } from '../api/scheme'
import type { MusicItem } from '../api/music'

/**
 * 页面级音乐播放器。
 *
 * 只在音乐页实例化：`onBeforeUnmount` 会停播并释放 Blob URL，满足「离开页面
 * 即停」。不写入全局 store，因此不会影响书架/阅读/直播等其它页面。
 */
export function useMusicPlayer() {
  const store = useMusicStore()
  const libraryStore = useMusicLibraryStore()
  const audio = new Audio()
  audio.preload = 'auto'

  const queue = ref<MusicItem[]>([])
  const index = ref(-1)
  const playing = ref(false)
  const loading = ref(false)
  const currentTime = ref(0)
  const duration = ref(0)
  const volume = ref(readVolume())
  const errorText = ref('')

  audio.volume = volume.value

  const current = computed<MusicItem | null>(() => queue.value[index.value] ?? null)

  let loadToken = 0
  let objectUrl: string | null = null

  // 听歌时长: 每秒累加, 满 30 秒落库一次; 切歌/离页时补写剩余部分。
  let pendingSeconds = 0
  let pendingItem: MusicItem | null = null
  let historyTimer: number | undefined

  function flushHistory(finished = false) {
    const item = pendingItem
    const seconds = Math.floor(pendingSeconds)
    pendingSeconds -= seconds
    if (!item || (seconds <= 0 && !finished)) return
    void libraryStore.record(item, seconds, finished).catch(() => undefined)
  }

  function readVolume() {
    const raw = Number(localStorage.getItem('music-volume'))
    return Number.isFinite(raw) && raw >= 0 && raw <= 1 ? raw : 0.8
  }

  function setVolume(value: number) {
    const next = Math.min(1, Math.max(0, value))
    volume.value = next
    audio.volume = next
    localStorage.setItem('music-volume', String(next))
  }

  function releaseObjectUrl() {
    if (objectUrl) {
      URL.revokeObjectURL(objectUrl)
      objectUrl = null
    }
  }

  async function playQueue(items: MusicItem[], startIndex = 0) {
    if (!items.length) return
    queue.value = [...items]
    index.value = Math.min(Math.max(0, startIndex), items.length - 1)
    await loadCurrent()
  }

  async function loadCurrent() {
    const item = current.value
    if (!item) return
    // 切歌前把上一首的剩余时长补写, 避免短时间试听被吞掉。
    flushHistory(false)
    pendingItem = item
    pendingSeconds = 0
    const token = ++loadToken
    errorText.value = ''
    loading.value = true
    try {
      let src: string
      if (item.platform === 'local') {
        // 本地曲库: 走 reader:// 白名单路由, 直接以文件字节播放。
        const path = String(item.localPath || item.id)
        src = `${readerOrigin}/musicLocal?path=${encodeURIComponent(path)}`
      } else {
        const source = await store.mediaSource(item)
        if (token !== loadToken) return
        src = source.url
        const headers = source.headers || {}
        if (Object.keys(headers).length > 0) {
          // 需要 Referer/UA 的音源: 交给 reader:// 代理取回整块字节后转 Blob。
          const params = new URLSearchParams({
            u: source.url,
            h: JSON.stringify(headers),
          })
          const response = await fetch(`${readerOrigin}/musicStream?${params.toString()}`)
          if (token !== loadToken) return
          if (!response.ok) {
            throw new Error(`取流失败（HTTP ${response.status}），该歌曲可能需要登录`)
          }
          const blob = await response.blob()
          if (token !== loadToken) return
          objectUrl = URL.createObjectURL(blob)
          src = objectUrl
        }
      }
      // 切到直链/本地文件时释放上一首的 Blob URL; 刚创建的这个要保留。
      if (src !== objectUrl) releaseObjectUrl()
      audio.src = src
      audio.load()
      await audio.play()
      if (token === loadToken) playing.value = true
    } catch (e) {
      if (token !== loadToken) return
      errorText.value = (e as Error).message || '播放失败'
      playing.value = false
    } finally {
      if (token === loadToken) loading.value = false
    }
  }

  function toggle() {
    if (!current.value) return
    if (audio.paused) {
      void audio.play().catch((e: Error) => {
        errorText.value = e.message || '播放失败'
      })
    } else {
      audio.pause()
    }
  }

  function next() {
    if (index.value + 1 >= queue.value.length) {
      audio.pause()
      return
    }
    index.value += 1
    void loadCurrent()
  }

  function prev() {
    if (index.value <= 0) return
    index.value -= 1
    void loadCurrent()
  }

  function seek(seconds: number) {
    if (!Number.isFinite(seconds)) return
    audio.currentTime = Math.max(0, seconds)
    currentTime.value = audio.currentTime
  }

  function stop() {
    loadToken += 1
    audio.pause()
    audio.removeAttribute('src')
    audio.load()
    playing.value = false
    loading.value = false
    currentTime.value = 0
    duration.value = 0
    releaseObjectUrl()
  }

  function destroy() {
    if (historyTimer !== undefined) {
      window.clearInterval(historyTimer)
      historyTimer = undefined
    }
    flushHistory(false)
    stop()
    queue.value = []
    index.value = -1
    audio.onloadedmetadata = null
    audio.ontimeupdate = null
    audio.onplay = null
    audio.onpause = null
    audio.onended = null
    audio.onerror = null
  }

  audio.onloadedmetadata = () => {
    duration.value = Number.isFinite(audio.duration) ? audio.duration : 0
  }
  audio.ontimeupdate = () => {
    currentTime.value = audio.currentTime
  }
  audio.onplay = () => {
    playing.value = true
  }
  audio.onpause = () => {
    playing.value = false
  }
  audio.onended = () => {
    playing.value = false
    flushHistory(true)
    next()
  }
  audio.onerror = () => {
    if (!audio.src) return
    errorText.value = '音频加载失败：格式不受当前环境支持，或链接已失效'
    playing.value = false
  }

  onBeforeUnmount(destroy)

  historyTimer = window.setInterval(() => {
    if (playing.value && current.value) pendingSeconds += 1
    if (pendingSeconds >= 30) flushHistory(false)
  }, 1000)

  return {
    queue,
    index,
    current,
    playing,
    loading,
    currentTime,
    duration,
    volume,
    errorText,
    playQueue,
    toggle,
    next,
    prev,
    seek,
    setVolume,
    stop,
  }
}
