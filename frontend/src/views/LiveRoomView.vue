<template>
  <div class="live-room-view">
    <header class="room-head">
      <button class="back-btn" @click="goBack">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M15 18l-6-6 6-6" />
        </svg>
        返回直播
      </button>
      <h1 class="head-title" :title="detail?.title">{{ detail?.title || '正在加载…' }}</h1>
      <div class="room-head-actions">
        <button class="follow-btn" :class="{ active: isFollowed }" @click="toggleFollow">
          {{ isFollowed ? '已关注' : '关注' }}
        </button>
        <span class="room-state" :class="{ live: detail?.isLive }">
          {{ detail?.isLive ? '直播中' : '未开播' }}
        </span>
      </div>
    </header>

    <div class="room-body">
      <!-- 左：播放器 + 操作行 -->
      <div class="player-column">
        <div ref="playerShellRef" class="player-shell">
          <video
            ref="videoRef"
            class="player"
            :style="videoStyle"
            playsinline
            muted
            @playing="onPlaying"
            @waiting="markBuffering"
            @pause="onPause"
            @play="onPlay"
          ></video>

          <div v-if="floatingDanmaku" class="danmaku-layer" aria-hidden="true" :style="danmakuLayerStyle">
            <span
              v-for="item in floatingItems"
              :key="item.id"
              class="danmaku-item"
              :class="item.kind"
              :style="{
                top: `${item.track * danmakuLaneHeight}px`,
                color: item.color || undefined,
                animationDuration: `${liveSettings.danmakuSpeed}s`,
              }"
            >
              <template v-if="item.kind === 'chat'">{{ item.content }}</template>
              <template v-else-if="item.kind === 'enter'">{{ item.user }} 进入直播间</template>
              <template v-else>{{ item.user }} {{ item.content }}</template>
            </span>
          </div>

          <div v-if="status !== 'playing'" class="player-overlay">
            <template v-if="status === 'loading'">
              <span class="spinner" aria-hidden="true"></span>
              <p>正在连接直播流…</p>
            </template>
            <template v-else-if="status === 'offline'">
              <p>主播当前未开播</p>
            </template>
            <template v-else>
              <p>{{ errorMsg || '直播流播放失败' }}</p>
              <button class="state-btn" @click="load">重新连接</button>
            </template>
          </div>

          <!-- 浮层控制条：对齐 Simple Live 的播放器底栏 -->
          <div class="player-bar">
            <button class="bar-btn" :title="paused ? '播放' : '暂停'" @click="togglePlay">
              <svg v-if="paused" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
              <svg v-else viewBox="0 0 24 24" fill="currentColor"><path d="M7 5h4v14H7zM13 5h4v14h-4z" /></svg>
            </button>
            <button
              v-if="danmakuSupported"
              class="bar-btn"
              :class="{ off: !danmakuOn }"
              title="弹幕开关"
              @click="toggleDanmaku"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 12a8 8 0 0 1-8 8H7l-4 3v-5.5A8 8 0 1 1 21 12z" />
              </svg>
            </button>
            <button class="bar-btn" :title="muted ? '取消静音' : '静音'" @click="toggleMute">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M11 5 6 9H3v6h3l5 4z" />
                <path v-if="!muted" d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13" />
                <path v-else d="m16 9 5 6M21 9l-5 6" />
              </svg>
            </button>
            <span class="bar-time">{{ watchedText }}</span>

            <span class="bar-spacer"></span>

            <button
              v-if="streams.length > 1"
              class="bar-text-btn"
              :title="'画质：' + currentQuality"
              @click="sideTab = 'settings'"
            >
              {{ currentQuality }}
            </button>
            <button class="bar-btn" title="全屏" @click="toggleFullscreen">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M8 3H5a2 2 0 0 0-2 2v3M16 3h3a2 2 0 0 1 2 2v3M16 21h3a2 2 0 0 0 2-2v-3M8 21H5a2 2 0 0 1-2-2v-3" />
              </svg>
            </button>
          </div>
        </div>

        <div class="room-actions">
          <button class="action-btn" @click="load">刷新</button>
          <button class="action-btn" :class="{ active: isFollowed }" @click="toggleFollow">
            {{ isFollowed ? '已关注' : '关注' }}
          </button>
          <button class="action-btn" @click="copyRoomLink">复制链接</button>
          <span class="action-spacer"></span>
          <span v-if="detail?.areaName" class="action-area">{{ detail.areaName }}</span>
        </div>
      </div>

      <!-- 右：常驻侧栏（聊天 / 关注 / 设置） -->
      <aside class="room-side">
        <header v-if="detail" class="side-head">
          <img
            v-if="detail.avatar"
            class="side-avatar"
            :src="detail.avatar"
            :alt="detail.userName"
            referrerpolicy="no-referrer"
          />
          <span v-else class="side-avatar placeholder">{{ (detail.userName || '?').slice(0, 1) }}</span>
          <div class="side-head-text">
            <strong>{{ detail.userName }}</strong>
            <small>{{ platformName }} · {{ formatOnline(detail.online) }} 人气</small>
          </div>
        </header>

        <nav class="side-tabs">
          <button
            v-for="tab in sideTabs"
            :key="tab.key"
            class="side-tab"
            :class="{ active: sideTab === tab.key }"
            @click="sideTab = tab.key"
          >
            {{ tab.label }}
          </button>
        </nav>

        <div class="side-body">
          <!-- 聊天：弹幕列表 -->
          <div v-if="sideTab === 'chat'" ref="chatRef" class="chat-list">
            <p v-if="danmakuError" class="chat-error">{{ danmakuError }}</p>
            <p v-else-if="!danmakuSupported" class="chat-empty">
              该平台暂未接入弹幕，画面与统计不受影响。
            </p>
            <p v-else-if="!chatMessages.length" class="chat-empty">正在连接弹幕…</p>
            <div
              v-for="message in chatMessages"
              :key="message.id"
              class="chat-row"
              :class="message.kind"
            >
              <template v-if="message.kind === 'chat'">
                <span class="chat-user">{{ message.user }}</span>
                <span class="chat-text" :style="message.color ? { color: message.color } : undefined">
                  {{ message.content }}
                </span>
              </template>
              <template v-else-if="message.kind === 'enter'">
                <span class="chat-system">{{ message.user }} 进入直播间</span>
              </template>
              <template v-else>
                <span class="chat-system">{{ message.user }} {{ message.content }}</span>
              </template>
            </div>
          </div>

          <!-- 关注 -->
          <div v-else-if="sideTab === 'follow'" class="side-panel">
            <button class="side-primary" :class="{ active: isFollowed }" @click="toggleFollow">
              {{ isFollowed ? '取消关注' : '关注主播' }}
            </button>
            <p class="side-hint">
              {{ isFollowed ? '已加入「直播 → 关注」列表，下次开播可在这里直接进入。' : '关注后可在直播首页的「关注」里快速找到这个直播间。' }}
            </p>
          </div>

          <!-- 设置：画面与画质 -->
          <div v-else class="side-panel">
            <section class="setting-group">
              <h3>画面</h3>
              <div class="setting-options">
                <button
                  v-for="option in fitOptions"
                  :key="option.value"
                  class="setting-chip"
                  :class="{ active: fitMode === option.value }"
                  @click="fitMode = option.value"
                >
                  {{ option.label }}
                </button>
              </div>
              <div class="setting-options">
                <button class="setting-chip" @click="rotateVideo">旋转 {{ rotate }}°</button>
                <button
                  class="setting-chip"
                  :class="{ active: mirrored }"
                  @click="mirrored = !mirrored"
                >
                  镜像
                </button>
              </div>
            </section>

            <section v-if="danmakuSupported" class="setting-group">
              <h3>弹幕</h3>
              <div class="setting-options">
                <button class="setting-chip" :class="{ active: floatingDanmaku }" @click="floatingDanmaku = !floatingDanmaku">
                  画面飘屏
                </button>
                <button class="setting-chip" :class="{ active: !floatingDanmaku }" @click="floatingDanmaku = !floatingDanmaku">
                  仅列表
                </button>
              </div>
            </section>

            <section v-if="streams.length" class="setting-group">
              <h3>画质 / 线路</h3>
              <div class="setting-options">
                <button
                  v-for="(stream, index) in streams"
                  :key="stream.url"
                  class="setting-chip"
                  :class="{ active: activeIndex === index }"
                  @click="switchStream(index)"
                >
                  {{ stream.quality }}
                </button>
              </div>
            </section>

            <section v-if="detail?.description" class="setting-group">
              <h3>主播简介</h3>
              <p class="side-desc">{{ detail.description }}</p>
            </section>
          </div>
        </div>
      </aside>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Hls from 'hls.js'
import flvjs from 'flv.js'
import { useLiveStore } from '../stores/live'
import { useLiveSettingsStore } from '../stores/liveSettings'
import { addLiveStats } from '../api/liveStats'
import { startLiveDanmaku, stopLiveDanmaku, type DanmakuMessage } from '../api/liveDanmaku'
import type { LiveRoomDetail, LiveStream } from '../api/live'

type PlayerStatus = 'loading' | 'playing' | 'error' | 'offline'

const route = useRoute()
const router = useRouter()
const store = useLiveStore()
const liveSettings = useLiveSettingsStore()

const videoRef = ref<HTMLVideoElement | null>(null)
const detail = ref<LiveRoomDetail | null>(null)
const streams = ref<LiveStream[]>([])
const activeIndex = ref(0)
const status = ref<PlayerStatus>('loading')
const errorMsg = ref('')
const muted = ref(true)

let hls: Hls | null = null
let flv: flvjs.Player | null = null

/** 观看时长上报：播放中用固定间隔累加，离开页面/暂停时补一次。 */
let watchTimer: number | undefined
let watchTickAt = 0
let sessionCounted = false

type DanmakuItem = DanmakuMessage & { id: number; track: number }
const DANMAKU_LANES = 6
const CHAT_LIMIT = 160
let danmakuSeq = 0
let danmakuLane = 0
/** 画面上飘的弹幕(短生命周期)。 */
const floatingItems = ref<DanmakuItem[]>([])
/** 右侧聊天列表(长生命周期, 上限 CHAT_LIMIT)。 */
const chatMessages = ref<DanmakuItem[]>([])
const chatRef = ref<HTMLElement | null>(null)
/** 弹幕开关持久化在 liveSettings store, 播放页切换与设置抽屉共用同一份。 */
const danmakuOn = computed({
  get: () => liveSettings.danmakuOn,
  set: (value: boolean) => liveSettings.setDanmakuOn(value),
})
const floatingDanmaku = ref(true)
const danmakuError = ref('')
let danmakuRunning = false
const danmakuTimers = new Set<number>()

const playerShellRef = ref<HTMLElement | null>(null)
const paused = ref(false)
const fitMode = ref<'contain' | 'cover' | 'fill'>('contain')
const rotate = ref(0)
const mirrored = ref(false)
const sideTab = ref<'chat' | 'follow' | 'settings'>('chat')
const watchedSeconds = ref(0)
const copied = ref(false)
let watchDisplayTimer: number | undefined

const fitOptions: Array<{ value: 'contain' | 'cover' | 'fill'; label: string }> = [
  { value: 'contain', label: '适应' },
  { value: 'cover', label: '裁剪' },
  { value: 'fill', label: '拉伸' },
]

const sideTabs = [
  { key: 'chat' as const, label: '聊天' },
  { key: 'follow' as const, label: '关注' },
  { key: 'settings' as const, label: '设置' },
]

const videoStyle = computed(() => ({
  objectFit: fitMode.value,
  transform: `rotate(${rotate.value}deg) scaleX(${mirrored.value ? -1 : 1})`,
}))

/** 飘屏层: 高度占比限制弹幕只落在画面上部, 透明度与字号来自直播设置。 */
const danmakuLayerStyle = computed<Record<string, string | number>>(() => ({
  height: `${liveSettings.danmakuArea}%`,
  opacity: liveSettings.danmakuOpacity,
  '--danmaku-font-size': `${liveSettings.danmakuFontSize}px`,
}))
/** 轨道间距随字号缩放, 避免大字号弹幕上下重叠。 */
const danmakuLaneHeight = computed(() => Math.round(liveSettings.danmakuFontSize * 1.75))

const currentQuality = computed(() => streams.value[activeIndex.value]?.quality || '默认')

const watchedText = computed(() => {
  const total = watchedSeconds.value
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  const pad = (value: number) => String(value).padStart(2, '0')
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`
})

const roomId = String(route.query.roomId || '')
const platform = String(route.query.platform || store.platform || 'bilibili')

const isFollowed = computed(() => store.followedKeys.has(`${platform}:${roomId}`))
/** 弹幕已接入 B 站/虎牙/斗鱼；其它平台先不做无效连接。 */
const danmakuSupported = computed(() => ['bilibili', 'huya', 'douyu', 'douyin'].includes(platform))
const platformName = computed(
  () => store.platforms.find((item) => item.id === platform)?.name || platform,
)

async function toggleFollow() {
  if (!detail.value) return
  try {
    await store.toggleFollow({
      platform,
      roomId,
      roomName: detail.value.title,
      anchorName: detail.value.userName,
      avatar: detail.value.avatar,
    })
  } catch (e) {
    errorMsg.value = (e as Error).message || '关注操作失败'
  }
}

function formatOnline(count: number) {
  if (count >= 10000) return `${(count / 10000).toFixed(1)}万`
  return String(count)
}

function pushDanmaku(messages: DanmakuMessage[]) {
  for (const message of messages) {
    if (message.kind === 'error') {
      errorMsg.value = message.content
      continue
    }
    const item: DanmakuItem = {
      ...message,
      id: ++danmakuSeq,
      track: danmakuLane++ % DANMAKU_LANES,
    }
    // 列表与飘屏共用同一份数据: 列表保留更久, 飘屏滚出画面(动画时长)后移除。
    chatMessages.value.push(item)
    if (chatMessages.value.length > CHAT_LIMIT) {
      chatMessages.value = chatMessages.value.slice(-CHAT_LIMIT)
    }
    if (danmakuOn.value && floatingDanmaku.value) {
      floatingItems.value.push(item)
      if (floatingItems.value.length > 40) {
        floatingItems.value = floatingItems.value.slice(-40)
      }
      const timer = window.setTimeout(() => {
        floatingItems.value = floatingItems.value.filter((entry) => entry.id !== item.id)
        danmakuTimers.delete(timer)
      }, liveSettings.danmakuSpeed * 1000)
      danmakuTimers.add(timer)
    }
  }
  void nextTick(scrollChatToBottom)
}

/** 聊天列表保持贴底；用户手动上翻时不强行拽回。 */
function scrollChatToBottom() {
  const list = chatRef.value
  if (!list) return
  const nearBottom = list.scrollHeight - list.scrollTop - list.clientHeight < 60
  if (nearBottom) list.scrollTop = list.scrollHeight
}

async function startDanmaku() {
  if (!danmakuSupported.value || danmakuRunning || !danmakuOn.value) return
  danmakuRunning = true
  danmakuError.value = ''
  try {
    await startLiveDanmaku(platform, roomId, pushDanmaku)
  } catch (e) {
    danmakuRunning = false
    // 弹幕不可用不影响看直播, 但要把原因显示在聊天面板里(不能只塞进播放器覆盖层)。
    danmakuError.value = (e as Error).message || '弹幕连接失败'
  }
}

function stopDanmaku() {
  if (!danmakuRunning) return
  danmakuRunning = false
  void stopLiveDanmaku(platform, roomId).catch(() => undefined)
}

function clearDanmaku() {
  for (const timer of danmakuTimers) window.clearTimeout(timer)
  danmakuTimers.clear()
  floatingItems.value = []
  chatMessages.value = []
}

function toggleDanmaku() {
  danmakuOn.value = !danmakuOn.value
  if (danmakuOn.value) {
    void startDanmaku()
  } else {
    stopDanmaku()
    floatingItems.value = []
  }
}

function togglePlay() {
  const video = videoRef.value
  if (!video) return
  if (video.paused) void video.play().catch(() => undefined)
  else video.pause()
}

function onPlay() {
  paused.value = false
}

function onPause() {
  paused.value = true
}

function rotateVideo() {
  rotate.value = (rotate.value + 90) % 360
}

function toggleFullscreen() {
  const shell = playerShellRef.value
  if (!shell) return
  if (document.fullscreenElement) {
    void document.exitFullscreen().catch(() => undefined)
    return
  }
  void shell.requestFullscreen().catch(() => undefined)
}

/** 复制直播间网页地址（不是内部路由），方便分享给不在用本软件的人。 */
async function copyRoomLink() {
  const link =
    platform === 'douyu'
      ? `https://www.douyu.com/${roomId}`
      : platform === 'huya'
        ? `https://www.huya.com/${roomId}`
        : platform === 'douyin'
          ? `https://live.douyin.com/${roomId}`
          : `https://live.bilibili.com/${roomId}`
  try {
    await navigator.clipboard.writeText(link)
    copied.value = true
    window.setTimeout(() => (copied.value = false), 1600)
  } catch {
    // 剪贴板不可用时退化为显示链接，让用户自己复制。
    errorMsg.value = `复制失败，链接：${link}`
  }
}

function destroyPlayer() {
  hls?.destroy()
  hls = null
  if (flv) {
    flv.pause()
    flv.unload()
    flv.detachMediaElement()
    flv.destroy()
    flv = null
  }
  const video = videoRef.value
  if (video) {
    video.removeAttribute('src')
    video.load()
  }
}

function markBuffering() {
  if (status.value === 'playing') status.value = 'loading'
}

function onPlaying() {
  status.value = 'playing'
  paused.value = false
  startWatchClock()
}

function reportWatch(seconds: number, sessions: number) {
  return addLiveStats({
    seconds,
    sessions,
    platform,
    roomId,
    roomName: detail.value?.title || '',
    anchorName: detail.value?.userName || '',
  })
}

/** 开始计时：首次出画面时记一次"场次"，之后每 30 秒累加实际播放时长。 */
function startWatchClock() {
  if (watchTimer !== undefined) return
  watchTickAt = Date.now()
  if (!sessionCounted) {
    sessionCounted = true
    void reportWatch(0, 1).catch(() => undefined)
  }
  // 控制条上的观看时长：每秒递增，暂停/缓冲时停住。
  watchDisplayTimer = window.setInterval(() => {
    if (status.value === 'playing' && !paused.value) watchedSeconds.value += 1
  }, 1000)
  watchTimer = window.setInterval(() => {
    if (status.value !== 'playing' || paused.value) {
      // 缓冲/暂停期间不计时，直接从当前时刻重新开始。
      watchTickAt = Date.now()
      return
    }
    void flushWatch()
  }, 30_000)
}

async function flushWatch() {
  const now = Date.now()
  const seconds = Math.floor((now - watchTickAt) / 1000)
  if (seconds < 5) return
  watchTickAt = now
  try {
    await reportWatch(seconds, 0)
  } catch {
    // 统计失败不影响观看。
  }
}

function stopWatchClock() {
  if (watchTimer !== undefined) {
    window.clearInterval(watchTimer)
    watchTimer = undefined
  }
  if (watchDisplayTimer !== undefined) {
    window.clearInterval(watchDisplayTimer)
    watchDisplayTimer = undefined
  }
  void flushWatch()
}

async function load() {
  if (!roomId) {
    status.value = 'error'
    errorMsg.value = '缺少直播间号'
    return
  }
  status.value = 'loading'
  errorMsg.value = ''
  // 先对齐平台, 再用 store 里的取流/详情方法(它们读的是 store.platform)。
  if (store.platform !== platform) store.platform = platform
  try {
    const [roomDetail, streamInfo] = await Promise.all([
      store.fetchRoomDetail(roomId),
      store.fetchStreamUrls(roomId),
    ])
    detail.value = roomDetail
    /**
     * 只有部分平台的流能在 WebView 里直接播:
     * - 虎牙/哔哩哔哩的 flv 校验 Referer(浏览器禁止脚本改这个头) → 只用 hls;
     * - 斗鱼只给 flv, 但其 CDN 不校验 Referer 且回了 `Access-Control-Allow-Origin: *`,
     *   交给 flv.js 播; 抖音的 flv 地址自带鉴权参数, 同样不校验 Referer。
     */
    streams.value = streamInfo.streams.filter(
      (item) => item.format === 'hls' || ((platform === 'douyu' || platform === 'douyin') && item.format === 'flv'),
    )
    if (!streamInfo.isLive || !streams.value.length) {
      status.value = 'offline'
      return
    }
    activeIndex.value = 0
    await nextTick()
    playCurrent()
    void startDanmaku()
  } catch (e) {
    status.value = 'error'
    errorMsg.value = (e as Error).message || '加载直播间失败'
  }
}

function playCurrent() {
  const stream = streams.value[activeIndex.value]
  const video = videoRef.value
  if (!stream?.url || !video) return
  destroyPlayer()
  status.value = 'loading'

  if (stream.format === 'flv') {
    playFlv(stream.url, video)
    return
  }

  if (Hls.isSupported()) {
    const instance = new Hls({
      lowLatencyMode: true,
      liveSyncDurationCount: 3,
      enableWorker: true,
    })
    hls = instance
    instance.on(Hls.Events.MANIFEST_PARSED, () => {
      video.play().catch(() => undefined)
    })
    instance.on(Hls.Events.ERROR, (_event, data) => {
      if (!data.fatal) return
      status.value = 'error'
      errorMsg.value = `播放中断（${data.type}: ${data.details}）`
      instance.destroy()
      hls = null
    })
    instance.loadSource(stream.url)
    instance.attachMedia(video)
    return
  }

  if (video.canPlayType('application/vnd.apple.mpegurl')) {
    video.src = stream.url
    video.play().catch(() => undefined)
    return
  }

  status.value = 'error'
  errorMsg.value = '当前环境不支持 HLS 播放'
}

/** FLV: 交给 flv.js 转封装后走 MSE。斗鱼的 CDN 已放行跨域。 */
function playFlv(url: string, video: HTMLVideoElement) {
  if (!flvjs.isSupported()) {
    status.value = 'error'
    errorMsg.value = '当前环境不支持 FLV 播放'
    return
  }
  const player = flvjs.createPlayer(
    { type: 'flv', url, isLive: true, hasAudio: true, hasVideo: true },
    // 直播场景不留缓冲，否则延迟会持续累积。
    { enableStashBuffer: false, autoCleanupSourceBuffer: true, lazyLoad: false },
  )
  flv = player
  player.on(flvjs.Events.ERROR, (type: string, detail: string) => {
    status.value = 'error'
    errorMsg.value = `播放中断（${type}: ${detail}）`
  })
  player.attachMediaElement(video)
  player.load()
  player.play()
}

function switchStream(index: number) {
  if (index === activeIndex.value) return
  activeIndex.value = index
  playCurrent()
}

function toggleMute() {
  const video = videoRef.value
  if (!video) return
  muted.value = !muted.value
  video.muted = muted.value
}

function goBack() {
  if (window.history.length > 1) router.back()
  else router.push({ name: 'live' })
}

onMounted(() => {
  void store.loadFollows()
  void load()
})

onBeforeUnmount(() => {
  stopWatchClock()
  stopDanmaku()
  clearDanmaku()
  destroyPlayer()
})
</script>

<style scoped>
.live-room-view {
  height: 100%;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  max-width: var(--content-max-width);
  margin: 0 auto;
  padding: 0 var(--space-6) var(--space-4);
}

.room-head {
  padding: var(--space-3) 0;
  display: flex;
  align-items: center;
  gap: var(--space-4);
  flex-shrink: 0;
}

.head-title {
  flex: 1;
  min-width: 0;
  margin: 0;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--text-base);
  font-weight: 650;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.back-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px 6px 8px;
  border-radius: var(--radius-full);
  border: 1px solid transparent;
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  flex-shrink: 0;
  transition: all var(--duration-fast) var(--ease-out);
}

.back-btn svg {
  width: 18px;
  height: 18px;
}

.back-btn:hover {
  border-color: var(--color-border-light);
  background: var(--color-bg-elevated);
  color: var(--color-text);
}

.room-state {
  padding: 4px 10px;
  border-radius: var(--radius-full);
  background: var(--color-bg-sunken);
  color: var(--color-text-tertiary);
  font-size: 12px;
  font-weight: 600;
}

.room-state.live {
  background: var(--color-primary-bg);
  color: var(--color-primary);
}

.room-head-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.follow-btn {
  padding: 5px 12px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
  font-size: 12px;
  transition: all var(--duration-fast) var(--ease-out);
}

.follow-btn:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.follow-btn.active {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

/* ── 两栏：左播放器 + 右常驻侧栏 ── */
.room-body {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: var(--space-4);
  align-items: stretch;
}

.player-column {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-height: 0;
}

.player-shell {
  position: relative;
  width: 100%;
  aspect-ratio: 16 / 9;
  max-height: calc(100vh - 220px);
  overflow: hidden;
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-light);
  background: #000;
  flex-shrink: 0;
}

.player-shell:fullscreen {
  max-height: none;
  border-radius: 0;
  border: none;
}

.player {
  width: 100%;
  height: 100%;
  display: block;
  background: #000;
  transition: transform var(--duration-fast) var(--ease-out);
}

/* 弹幕层：绝对定位在画面上，只做 left 动画，且不吃鼠标事件。 */
.danmaku-layer {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
  z-index: 2;
}

.danmaku-item {
  position: absolute;
  left: 100%;
  white-space: nowrap;
  color: #fff;
  /* 字号由直播设置通过 --danmaku-font-size 注入, 动画时长按内联样式(滚动速度)。 */
  font-size: var(--danmaku-font-size, 14px);
  font-weight: 500;
  line-height: 1.5;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.85), 0 0 1px rgba(0, 0, 0, 0.9);
  animation: danmaku-run 9s linear forwards;
  will-change: left;
}

.danmaku-item.gift,
.danmaku-item.superchat {
  padding: 1px 8px;
  border-radius: var(--radius-full);
  background: rgba(197, 111, 53, 0.85);
  font-weight: 600;
}

.danmaku-item.enter {
  color: rgba(255, 255, 255, 0.6);
  font-size: calc(var(--danmaku-font-size, 14px) * 0.8);
}

@keyframes danmaku-run {
  from {
    left: 100%;
  }
  to {
    left: -100%;
  }
}

.player-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  background: rgba(0, 0, 0, 0.66);
  color: rgba(255, 255, 255, 0.86);
  font-size: 13px;
}

.spinner {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.24);
  border-top-color: var(--color-primary);
  animation: live-spin 800ms linear infinite;
}

@keyframes live-spin {
  to {
    transform: rotate(360deg);
  }
}

.state-btn {
  padding: 7px 16px;
  border-radius: var(--radius-full);
  border: 1px solid rgba(255, 255, 255, 0.24);
  color: #fff;
  font-size: 13px;
}

/* 播放器浮层控制条 */
.player-bar {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 3;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  background: linear-gradient(to top, rgba(0, 0, 0, 0.72), rgba(0, 0, 0, 0));
  opacity: 0;
  transition: opacity var(--duration-normal) var(--ease-out);
}

.player-shell:hover .player-bar,
.player-shell:focus-within .player-bar {
  opacity: 1;
}

.bar-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: var(--radius-md);
  color: rgba(255, 255, 255, 0.88);
  transition: all var(--duration-fast) var(--ease-out);
}

.bar-btn svg {
  width: 17px;
  height: 17px;
}

.bar-btn:hover {
  background: rgba(255, 255, 255, 0.16);
  color: #fff;
}

.bar-btn.off {
  color: rgba(255, 255, 255, 0.42);
}

.bar-time {
  margin-left: 4px;
  color: rgba(255, 255, 255, 0.8);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.bar-spacer {
  flex: 1;
}

.bar-text-btn {
  padding: 4px 10px;
  border-radius: var(--radius-md);
  color: rgba(255, 255, 255, 0.88);
  font-size: 12px;
  font-weight: 500;
}

.bar-text-btn:hover {
  background: rgba(255, 255, 255, 0.16);
  color: #fff;
}

.room-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.action-btn {
  padding: 6px 14px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
  font-size: 12px;
  transition: all var(--duration-fast) var(--ease-out);
}

.action-btn:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.action-btn.active {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.action-spacer {
  flex: 1;
}

.action-area {
  padding: 3px 9px;
  border-radius: var(--radius-md);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-size: 12px;
  font-weight: 600;
}

/* ── 右侧栏 ── */
.room-side {
  width: 312px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
  overflow: hidden;
}

.side-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px;
  border-bottom: 1px solid var(--color-divider);
  flex-shrink: 0;
}

.side-avatar {
  width: 38px;
  height: 38px;
  border-radius: 50%;
  object-fit: cover;
  flex-shrink: 0;
}

.side-avatar.placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--color-bg-sunken);
  color: var(--color-text-tertiary);
  font-size: 15px;
}

.side-head-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.side-head-text strong {
  overflow: hidden;
  color: var(--color-text);
  font-size: 14px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.side-head-text small {
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.side-tabs {
  display: flex;
  border-bottom: 1px solid var(--color-divider);
  flex-shrink: 0;
}

.side-tab {
  flex: 1;
  padding: 9px 0;
  color: var(--color-text-tertiary);
  font-size: 13px;
  border-bottom: 2px solid transparent;
  transition: all var(--duration-fast) var(--ease-out);
}

.side-tab:hover {
  color: var(--color-text);
}

.side-tab.active {
  border-bottom-color: var(--color-primary);
  color: var(--color-primary);
  font-weight: 600;
}

.side-body {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.chat-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 7px;
  scrollbar-width: thin;
}

.chat-empty {
  margin: auto;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-align: center;
  line-height: 1.6;
  padding: 0 12px;
}

.chat-error {
  margin: 4px 0 auto;
  color: var(--color-danger, #c0392b);
  font-size: 12px;
  line-height: 1.6;
}

.chat-row {
  display: flex;
  gap: 6px;
  font-size: 13px;
  line-height: 1.5;
  word-break: break-word;
}

.chat-user {
  flex-shrink: 0;
  color: var(--color-primary);
  font-weight: 500;
}

.chat-user::after {
  content: '：';
  color: var(--color-text-tertiary);
  font-weight: 400;
}

.chat-text {
  min-width: 0;
  color: var(--color-text);
}

.chat-row.gift .chat-text,
.chat-row.superchat .chat-text {
  font-weight: 600;
}

.chat-system {
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.side-panel {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 14px 12px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.side-primary {
  padding: 9px 14px;
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-size: 13px;
  font-weight: 600;
}

.side-primary.active {
  background: transparent;
  color: var(--color-text-secondary);
  border-color: var(--color-border-light);
}

.side-hint {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: 12px;
  line-height: 1.65;
}

.setting-group h3 {
  margin: 0 0 8px;
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 600;
}

.setting-options {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.setting-options + .setting-options {
  margin-top: 8px;
}

.setting-chip {
  padding: 5px 11px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
  transition: all var(--duration-fast) var(--ease-out);
}

.setting-chip:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.setting-chip.active {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.side-desc {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: 12px;
  line-height: 1.7;
  white-space: pre-wrap;
}

/* 窄屏：侧栏收到下方，播放器占满宽度 */
@media (max-width: 1024px) {
  .room-side {
    width: 268px;
  }
}

@media (max-width: 900px) {
  .live-room-view {
    overflow: auto;
    padding: 0 var(--space-4) var(--space-4);
  }

  .room-body {
    flex-direction: column;
  }

  .player-shell {
    max-height: 46vh;
  }

  .room-side {
    width: 100%;
    min-height: 240px;
    max-height: 46vh;
  }
}

@media (max-width: 640px) {
  .head-title {
    font-size: 14px;
  }

  .player-shell {
    border-radius: var(--radius-md);
    max-height: 36vh;
  }
}
</style>
