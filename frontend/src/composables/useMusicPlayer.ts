import { computed, ref, watch } from 'vue'
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

// ─── localStorage 键名 ───
const PERSIST_KEY = 'music-player-state'

// ─── 模块级状态（全局单例） ───
let audio: HTMLAudioElement | null = null

const queue = ref<MusicItem[]>([])
const index = ref(-1)
const playing = ref(false)
const loading = ref(false)
const currentTime = ref(0)
const duration = ref(0)
const volume = ref(readVolume())
const errorText = ref('')
const effectiveQuality = ref('')
const swappedFrom = ref('')
const sleepMode = ref<'off' | 'countdown' | 'end-of-track'>('off')
const sleepRemaining = ref(0)

function ensureAudio(): HTMLAudioElement {
  if (!audio) {
    audio = new Audio()
    audio.preload = 'auto'
    audio.volume = volume.value
    // 事件绑定（用局部变量 a，避免 TS null 警告）
    const a = audio
    a.onloadedmetadata = () => {
      duration.value = Number.isFinite(a.duration) ? a.duration : 0
      if (pendingSeek > 0 && current.value) {
        a.currentTime = Math.min(pendingSeek, a.duration || pendingSeek)
        currentTime.value = a.currentTime
        pendingSeek = 0
      }
    }
    a.ontimeupdate = () => { currentTime.value = a.currentTime }
    a.onplay = () => { playing.value = true }
    a.onpause = () => { playing.value = false }
    a.onended = () => {
      playing.value = false
      flushHistory(true)
      if (sleepMode.value === 'end-of-track') {
        cancelSleep()
        stop()
        saveState()
        return
      }
      if (useMusicSettingsStore().playMode === 'repeat-one') {
        a.currentTime = 0
        currentTime.value = 0
        void a.play().catch((e: Error) => {
          errorText.value = e.message || '播放失败'
        })
        return
      }
      advance()
    }
    a.onerror = () => {
      if (!a.src) return
      errorText.value = '音频加载失败：格式不受当前环境支持，或链接已失效'
      playing.value = false
    }
  }
  return audio!
}

const current = computed<MusicItem | null>(() => queue.value[index.value] ?? null)

let loadToken = 0
let objectUrl: string | null = null
let initialized = false

let playedIndices: number[] = []
let shuffleHistory: number[] = []

let pendingSeconds = 0
let pendingItem: MusicItem | null = null
let historyTimer: number | undefined
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

function readVolume() {
  try {
    const raw = Number(localStorage.getItem('music-volume'))
    return Number.isFinite(raw) && raw >= 0 && raw <= 1 ? raw : 0.8
  } catch {
    return 0.8
  }
}

function flushHistory(finished = false) {
  const item = pendingItem
  const seconds = Math.floor(pendingSeconds)
  pendingSeconds -= seconds
  if (!item || (seconds <= 0 && !finished)) return
  void useMusicLibraryStore().record(item, seconds, finished).catch(() => undefined)
}

function releaseObjectUrl() {
  if (objectUrl) {
    URL.revokeObjectURL(objectUrl)
    objectUrl = null
  }
}

/** 持久化播放器状态到 localStorage（不含音源自适应 URL，会过期）。 */
function saveState() {
  try {
    const settings = useMusicSettingsStore()
    const state = {
      queue: queue.value,
      index: index.value,
      currentTime: currentTime.value,
      volume: volume.value,
      playMode: settings.playMode,
      playbackRate: settings.playbackRate,
      savedAt: Date.now(),
    }
    localStorage.setItem(PERSIST_KEY, JSON.stringify(state))
  } catch {
    // localStorage 可能已满或不可用，静默忽略。
  }
}

/** 从 localStorage 恢复播放器状态。返回 true 表示有恢复的状态。 */
function loadState(): boolean {
  try {
    const raw = localStorage.getItem(PERSIST_KEY)
    if (!raw) return false
    const state = JSON.parse(raw)
    if (!state || !Array.isArray(state.queue) || state.queue.length === 0) return false
    queue.value = state.queue as MusicItem[]
    index.value = Math.min(state.index ?? 0, queue.value.length - 1)
    volume.value = state.volume ?? 0.8
    ensureAudio().volume = volume.value
    const settings = useMusicSettingsStore()
    if (state.playMode) settings.playMode = state.playMode
    if (state.playbackRate) settings.playbackRate = state.playbackRate
    if (state.currentTime && Number.isFinite(state.currentTime)) {
      // seek 会在 loadCurrent + metadata loaded 后执行。
      pendingSeek = state.currentTime
    }
    // 恢复随机播放的历史，从当前 index 开始新一轮。
    playedIndices = [index.value]
    shuffleHistory = []
    return true
  } catch {
    return false
  }
}

/** 恢复时需要 seek 到的时间点（单位秒）。 */
let pendingSeek = 0

async function playQueue(items: MusicItem[], startIndex = 0) {
  if (!items.length) return
  queue.value = [...items]
  index.value = Math.min(Math.max(0, startIndex), items.length - 1)
  playedIndices = [index.value]
  shuffleHistory = []
  saveState()
  await loadCurrent()
}

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
  saveState()
}

function jumpTo(target: number) {
  if (target < 0 || target >= queue.value.length || target === index.value) return
  if (useMusicSettingsStore().playMode === 'shuffle') {
    shuffleHistory.push(index.value)
    playedIndices = [...playedIndices, target]
  }
  index.value = target
  saveState()
  void loadCurrent()
}

function removeAt(target: number) {
  if (target < 0 || target >= queue.value.length) return
  const removingCurrent = target === index.value
  queue.value = queue.value.filter((_, position) => position !== target)
  if (!queue.value.length) {
    stop()
    index.value = -1
    playedIndices = []
    shuffleHistory = []
    saveState()
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
  saveState()
}

function clearQueue() {
  stop()
  queue.value = []
  index.value = -1
  playedIndices = []
  shuffleHistory = []
  localStorage.removeItem(PERSIST_KEY)
}

/**
 * 按音质链逐档取流；全部失败时若配置了替代插件，就在替代音源上重搜再取。
 */
async function resolveSource(
  item: MusicItem,
): Promise<{ source: MusicMediaSource; swappedFrom: string }> {
  const store = useMusicStore()
  const settings = useMusicSettingsStore()
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
      const path = String(item.localPath || item.id)
      src = `${readerOrigin}/musicLocal?path=${encodeURIComponent(path)}`
    } else {
      const resolved = await resolveSource(item)
      if (token !== loadToken) return
      effectiveQuality.value = resolved.source.quality
      swappedFrom.value = resolved.swappedFrom
      if (resolved.swappedFrom) {
        useAppStore().showToast(
          `「${item.title}」已从 ${resolved.swappedFrom} 音源播放`,
          'success',
        )
      }
      const source = resolved.source
      src = source.url
      const headers = source.headers || {}
      if (Object.keys(headers).length > 0) {
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
    if (src !== objectUrl) releaseObjectUrl()
    ensureAudio().src = src
    ensureAudio().load()
    ensureAudio().playbackRate = useMusicSettingsStore().playbackRate
    const playPromise = ensureAudio().play()
    if (playPromise !== undefined) {
      await playPromise
    }
    if (token === loadToken) playing.value = true
    saveState()
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
  if (ensureAudio().paused) {
    const p = ensureAudio().play()
    if (p !== undefined) {
      void p.catch((e: Error) => {
        errorText.value = e.message || '播放失败'
      })
    }
  } else {
    ensureAudio().pause()
  }
}

function advance() {
  const mode = useMusicSettingsStore().playMode
  const target = nextTrackIndex(mode, index.value, queue.value.length, playedIndices)
  if (target < 0) {
    ensureAudio().pause()
    playing.value = false
    saveState()
    return
  }
  if (mode === 'shuffle') {
    shuffleHistory.push(index.value)
    playedIndices = [...playedIndices, target]
  }
  index.value = target
  saveState()
  void loadCurrent()
}

function next() { advance() }

function prev() {
  if (useMusicSettingsStore().playMode === 'shuffle' && shuffleHistory.length) {
    const target = shuffleHistory.pop() as number
    playedIndices = playedIndices.filter((entry) => entry !== target)
    index.value = target
    saveState()
    void loadCurrent()
    return
  }
  if (index.value <= 0) return
  index.value -= 1
  saveState()
  void loadCurrent()
}

function seek(seconds: number) {
  if (!Number.isFinite(seconds)) return
  ensureAudio().currentTime = Math.max(0, seconds)
  currentTime.value = ensureAudio().currentTime
  saveState()
}

function reload() {
  void loadCurrent()
}

function stop() {
  loadToken += 1
  ensureAudio().pause()
  ensureAudio().removeAttribute('src')
  ensureAudio().load()
  playing.value = false
  loading.value = false
  currentTime.value = 0
  duration.value = 0
  releaseObjectUrl()
}

/** 应用退出时的完整清理：停播 + 落库剩余时长 + 释放资源。 */
function shutdown() {
  clearSleepTimer()
  if (historyTimer !== undefined) {
    window.clearInterval(historyTimer)
    historyTimer = undefined
  }
  flushHistory(false)
  saveState()
  stop()
  queue.value = []
  index.value = -1
  ensureAudio().onloadedmetadata = null
  ensureAudio().ontimeupdate = null
  ensureAudio().onplay = null
  ensureAudio().onpause = null
  ensureAudio().onended = null
  ensureAudio().onerror = null
}

// ─── 定时关闭: 倒计时 ───
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
      saveState()
    }
  }, 1000)
}

function setSleepEndOfTrack() {
  clearSleepTimer()
  sleepMode.value = 'end-of-track'
  sleepRemaining.value = 0
}

function setVolume(value: number) {
  const next = Math.min(1, Math.max(0, value))
  volume.value = next
  ensureAudio().volume = next
  localStorage.setItem('music-volume', String(next))
}

// 每 30 秒落库一次听歌时长
function startHistoryTimer() {
  if (historyTimer !== undefined) return
  historyTimer = window.setInterval(() => {
    if (playing.value && current.value) pendingSeconds += 1
    if (pendingSeconds >= 30) flushHistory(false)
  }, 1000)
}

/**
 * 全局单例音乐播放器。
 *
 * 模块级状态 + Audio 对象，生命周期与应用一致。
 * 任何组件（App.vue / MusicView.vue / 未来的播放条 mini 模式）
 * 都可以调用 `useMusicPlayer()` 拿到同一份引用。
 */
export function useMusicPlayer() {
  if (!initialized) {
    initialized = true
    // 尝试从 localStorage 恢复上次的队列与位置（不会自动播放）
    loadState()
    startHistoryTimer()
    // 倍速同步
    watch(
      () => useMusicSettingsStore().playbackRate,
      (rate) => { if (audio) audio.playbackRate = rate },
    )
  }

  return {
    // 状态
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
    // 方法
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
    shutdown,
    saveState,
  }
}
