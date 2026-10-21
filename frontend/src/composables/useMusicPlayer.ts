import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useMusicStore } from '../stores/music'
import { useMusicLibraryStore } from '../stores/musicLibrary'
import { qualityChain, nextTrackIndex, useMusicSettingsStore } from '../stores/musicSettings'
import { useAppStore } from '../stores/app'
import { searchMusic, type MusicItem, type MusicMediaSource } from '../api/music'
import { readerOrigin } from '../api/scheme'

/**
 * 从替代音源的搜索结果里挑最像原曲的一首：标题归一后相同优先，否则取第一条。
 * 归一化只做小写、去空格、去括号补充说明，避免把「(Live)」这类版本差异算成不同曲。
 */
export function pickAlternative(candidates: MusicItem[], item: MusicItem): MusicItem | null {
  if (!candidates.length) return null
  const normalize = (value: string) =>
    value
      .toLowerCase()
      .replace(/\s+/g, '')
      .replace(/[（(【\[].*?[）)】\]]/g, '')
  const wanted = normalize(item.title || '')
  const matched = candidates.find((entry) => normalize(entry.title || '') === wanted)
  return matched ?? candidates[0]
}

/**
 * 页面级音乐播放器。
 *
 * 只在音乐页实例化：`onBeforeUnmount` 会停播并释放 Blob URL，满足「离开页面
 * 即停」。不写入全局 store，因此不会影响书架/阅读/直播等其它页面。
 */
export function useMusicPlayer() {
  const store = useMusicStore()
  const libraryStore = useMusicLibraryStore()
  const settings = useMusicSettingsStore()
  const appStore = useAppStore()
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
  /** 当前曲实际拿到的音质（插件返回的原值），失败降级后会是降级档位。 */
  const effectiveQuality = ref('')
  /** 发生换源时记录来源平台，供播放条提示。 */
  const swappedFrom = ref('')
  /** 定时关闭：off / 倒计时 / 播完当前曲。 */
  const sleepMode = ref<'off' | 'countdown' | 'end-of-track'>('off')
  const sleepRemaining = ref(0)

  audio.volume = volume.value

  const current = computed<MusicItem | null>(() => queue.value[index.value] ?? null)

  let loadToken = 0
  let objectUrl: string | null = null

  // 随机播放：playedIndices 记录这一轮已播的索引，shuffleHistory 供「上一首」回退。
  let playedIndices: number[] = []
  let shuffleHistory: number[] = []

  // 听歌时长: 每秒累加, 满 30 秒落库一次; 切歌/离页时补写剩余部分。
  let pendingSeconds = 0
  let pendingItem: MusicItem | null = null
  let historyTimer: number | undefined

  // 定时关闭的倒计时定时器。
  let sleepTimer: number | undefined

  function clearSleepTimer() {
    if (sleepTimer !== undefined) {
      window.clearInterval(sleepTimer)
      sleepTimer = undefined
    }
  }

  function cancelSleep() {
    clearSleepTimer()
    sleepMode.value = 'off'
    sleepRemaining.value = 0
  }

  /** 倒计时关闭：到点立即停播（队列保留，可手动继续）。 */
  function setSleepCountdown(minutes: number) {
    cancelSleep()
    const seconds = Math.max(1, Math.round(minutes * 60))
    sleepMode.value = 'countdown'
    sleepRemaining.value = seconds
    sleepTimer = window.setInterval(() => {
      sleepRemaining.value -= 1
      if (sleepRemaining.value <= 0) {
        cancelSleep()
        stop()
      }
    }, 1000)
  }

  /** 播完当前曲关闭：onended 里直接停，不再切下一首。 */
  function setSleepEndOfTrack() {
    clearSleepTimer()
    sleepMode.value = 'end-of-track'
    sleepRemaining.value = 0
  }

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
    playedIndices = [index.value]
    shuffleHistory = []
    await loadCurrent()
  }

  /** 下一首播放：插到当前曲之后；当前没有播放时直接开播。 */
  function playNext(item: MusicItem) {
    if (!queue.value.length || index.value < 0) {
      void playQueue([item], 0)
      return
    }
    const insertAt = index.value + 1
    queue.value = [
      ...queue.value.slice(0, insertAt),
      item,
      ...queue.value.slice(insertAt),
    ]
  }

  function jumpTo(target: number) {
    if (target < 0 || target >= queue.value.length || target === index.value) return
    if (settings.playMode === 'shuffle') {
      shuffleHistory.push(index.value)
      playedIndices = [...playedIndices, target]
    }
    index.value = target
    void loadCurrent()
  }

  /** 从队列移除一首；删到当前曲时由后一首顶上。 */
  function removeAt(target: number) {
    if (target < 0 || target >= queue.value.length) return
    const removingCurrent = target === index.value
    queue.value = queue.value.filter((_, position) => position !== target)
    if (!queue.value.length) {
      stop()
      index.value = -1
      playedIndices = []
      shuffleHistory = []
      return
    }
    if (target < index.value) {
      index.value -= 1
    } else if (removingCurrent) {
      index.value = Math.min(index.value, queue.value.length - 1)
      void loadCurrent()
    }
    const shift = (position: number) =>
      position === target ? -1 : position > target ? position - 1 : position
    playedIndices = playedIndices.map(shift).filter((position) => position >= 0)
    shuffleHistory = shuffleHistory.map(shift).filter((position) => position >= 0)
  }

  function clearQueue() {
    stop()
    queue.value = []
    index.value = -1
    playedIndices = []
    shuffleHistory = []
  }

  /**
   * 按音质链逐档取流；全部失败时若配置了替代插件，就在替代音源上重搜再取。
   * 返回带来源标记的播放地址。
   */
  async function resolveSource(
    item: MusicItem,
  ): Promise<{ source: MusicMediaSource; swappedFrom: string }> {
    const chain = qualityChain(settings.preferredQuality, settings.qualityOrder)
    let lastError: Error | null = null

    for (const quality of chain) {
      try {
        const source = await store.mediaSource(item, quality)
        if (source.url) {
          return {
            source: { ...source, quality: source.quality || quality },
            swappedFrom: '',
          }
        }
      } catch (error) {
        lastError = error as Error
      }
    }

    const alternative = settings.alternativeMap[item.platform]
    if (alternative) {
      const keyword = [item.title, item.artist].filter(Boolean).join(' ')
      const candidates = await searchMusic({
        platform: alternative,
        keyword,
        page: 1,
      }).catch(() => null)
      const target = pickAlternative(candidates?.items ?? [], item)
      if (target) {
        for (const quality of chain) {
          try {
            const source = await store.mediaSource(target, quality)
            if (source.url) {
              return {
                source: { ...source, quality: source.quality || quality },
                swappedFrom: alternative,
              }
            }
          } catch (error) {
            lastError = error as Error
          }
        }
      }
    }

    throw lastError ?? new Error('未找到可用的播放地址，该歌曲可能需要登录或无版权')
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
    effectiveQuality.value = ''
    swappedFrom.value = ''
    loading.value = true
    try {
      let src: string
      if (item.platform === 'local') {
        // 本地曲库: 走 reader:// 白名单路由, 直接以文件字节播放。
        const path = String(item.localPath || item.id)
        src = `${readerOrigin}/musicLocal?path=${encodeURIComponent(path)}`
      } else {
        const resolved = await resolveSource(item)
        if (token !== loadToken) return
        effectiveQuality.value = resolved.source.quality
        swappedFrom.value = resolved.swappedFrom
        if (resolved.swappedFrom) {
          appStore.showToast(
            `「${item.title}」已从 ${resolved.swappedFrom} 音源播放`,
            'success',
          )
        }
        const source = resolved.source
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
      audio.playbackRate = settings.playbackRate
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

  /** 手动/自动切下一首：随机模式按未播池取，其余按顺序；没有下一首就停住。 */
  function advance() {
    const mode = settings.playMode
    const target = nextTrackIndex(mode, index.value, queue.value.length, playedIndices)
    if (target < 0) {
      audio.pause()
      playing.value = false
      return
    }
    if (mode === 'shuffle') {
      shuffleHistory.push(index.value)
      playedIndices = [...playedIndices, target]
    }
    index.value = target
    void loadCurrent()
  }

  function next() {
    advance()
  }

  function prev() {
    if (settings.playMode === 'shuffle' && shuffleHistory.length) {
      const target = shuffleHistory.pop() as number
      playedIndices = playedIndices.filter((entry) => entry !== target)
      index.value = target
      void loadCurrent()
      return
    }
    if (index.value <= 0) return
    index.value -= 1
    void loadCurrent()
  }

  function seek(seconds: number) {
    if (!Number.isFinite(seconds)) return
    audio.currentTime = Math.max(0, seconds)
    currentTime.value = audio.currentTime
  }

  /** 重新取流播放当前曲（切换音质后调用）。 */
  function reload() {
    void loadCurrent()
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
    clearSleepTimer()
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
    if (sleepMode.value === 'end-of-track') {
      cancelSleep()
      stop()
      return
    }
    if (settings.playMode === 'repeat-one') {
      audio.currentTime = 0
      currentTime.value = 0
      void audio.play().catch((e: Error) => {
        errorText.value = e.message || '播放失败'
      })
      return
    }
    advance()
  }
  audio.onerror = () => {
    if (!audio.src) return
    errorText.value = '音频加载失败：格式不受当前环境支持，或链接已失效'
    playing.value = false
  }

  // 播放中调整倍速：<audio> 的 playbackRate 不会随设置自动同步。
  watch(
    () => settings.playbackRate,
    (rate) => {
      audio.playbackRate = rate
    },
  )

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
    effectiveQuality,
    swappedFrom,
    sleepMode,
    sleepRemaining,
    playQueue,
    playNext,
    jumpTo,
    removeAt,
    clearQueue,
    reload,
    setSleepCountdown,
    setSleepEndOfTrack,
    cancelSleep,
    toggle,
    next,
    prev,
    seek,
    setVolume,
    stop,
  }
}
