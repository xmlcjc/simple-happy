<template>
  <div class="live-room-view" :class="{ 'is-fullscreen': isFullscreen }">
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
        <span v-if="liveUptimeText" class="room-uptime" title="开播时长">已播 {{ liveUptimeText }}</span>
      </div>
    </header>

    <div ref="roomBodyRef" class="room-body">
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

          <!-- SC 卡片横幅: 展示期内的醒目留言, 到期自动移除 -->
          <div v-if="superchats.length" class="superchat-layer" aria-hidden="false">
            <div
              v-for="sc in superchats"
              :key="sc.id"
              class="sc-card"
              :style="{ '--sc-accent': scAccent(sc.price) }"
            >
              <img
                v-if="sc.avatar"
                class="sc-avatar"
                :src="sc.avatar"
                alt=""
                referrerpolicy="no-referrer"
              />
              <span v-else class="sc-avatar placeholder">{{ (sc.user || '?').slice(0, 1) }}</span>
              <div class="sc-body">
                <div class="sc-head">
                  <strong>{{ sc.user }}</strong>
                  <span class="sc-price">¥{{ sc.price }}</span>
                  <span class="sc-remain">{{ sc.remain }}s</span>
                </div>
                <p class="sc-message">{{ sc.message }}</p>
              </div>
              <button class="sc-close" title="关闭" @click="dismissSuperchat(sc.id)">×</button>
            </div>
          </div>

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
            <template v-else-if="status === 'reconnecting'">
              <span class="spinner" aria-hidden="true"></span>
              <p>重连中 {{ reconnectCount }}/{{ MAX_RECONNECT }}</p>
              <button class="state-btn" @click="manualSwitchLine">切换线路</button>
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
              v-if="streams.length"
              class="bar-text-btn"
              :title="'画质：' + currentQuality"
              @click="toggleQualitySheet"
            >
              {{ currentQuality }}
            </button>
            <button v-if="isFullscreen" class="bar-btn" title="设置" @click="openSideTab('settings')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="3" />
                <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.1-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.64 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V9a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.51 1z" />
              </svg>
            </button>
            <button class="bar-btn" title="全屏" @click="toggleFullscreen">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M8 3H5a2 2 0 0 0-2 2v3M16 3h3a2 2 0 0 1 2 2v3M16 21h3a2 2 0 0 0 2-2v-3M8 21H5a2 2 0 0 1-2-2v-3" />
              </svg>
            </button>
          </div>

          <!-- 清晰度选择: 按画质分组(同档多条 CDN 线路只显示一项, 容灾自动切换) -->
          <div v-if="qualitySheetOpen" class="quality-backdrop" @click="qualitySheetOpen = false"></div>
          <div v-if="qualitySheetOpen" class="quality-sheet" @click.stop>
            <div class="quality-sheet-head">清晰度</div>
            <button
              v-for="option in qualityOptions"
              :key="option.quality"
              class="quality-item"
              :class="{ active: activeQuality === option.quality }"
              @click="selectQuality(option)"
            >
              <span>{{ option.quality }}</span>
              <span v-if="activeQuality === option.quality" class="quality-check">✓</span>
            </button>
            <p v-if="showLoginQualityHint" class="quality-hint">
              未登录最高只有「超清」，可在 设置 → 直播 填写 B 站 Cookie 解锁原画 / 蓝光。
            </p>
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

      <!-- 右：常驻侧栏（聊天 / 关注 / 设置）；全屏时变为按需弹出的浮层 -->
      <aside class="room-side" :class="{ 'fs-open': isFullscreen && fsPanel !== null }">
        <button v-if="isFullscreen" class="side-close" title="关闭" @click="closeFsPanel">×</button>
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
            @click="openSideTab(tab.key)"
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
                <button
                  class="setting-chip"
                  :class="{ active: liveSettings.danmakuOn }"
                  @click="setDanmakuEnabled(true)"
                >
                  弹幕开
                </button>
                <button
                  class="setting-chip"
                  :class="{ active: !liveSettings.danmakuOn }"
                  @click="setDanmakuEnabled(false)"
                >
                  弹幕关
                </button>
                <button class="setting-chip" :class="{ active: floatingDanmaku }" @click="floatingDanmaku = true">
                  画面飘屏
                </button>
                <button class="setting-chip" :class="{ active: !floatingDanmaku }" @click="floatingDanmaku = false">
                  仅列表
                </button>
              </div>

              <div class="setting-slider-row">
                <label>显示区域</label>
                <div class="setting-options setting-options-inline">
                  <button
                    v-for="option in danmakuAreaOptions"
                    :key="option"
                    class="setting-chip"
                    :class="{ active: liveSettings.danmakuArea === option }"
                    @click="liveSettings.setDanmakuArea(option)"
                  >
                    {{ option }}%
                  </button>
                </div>
              </div>

              <div class="setting-slider-row">
                <label>不透明度</label>
                <input
                  type="range"
                  :min="danmakuOpacityRange.min"
                  :max="danmakuOpacityRange.max"
                  step="0.05"
                  :value="liveSettings.danmakuOpacity"
                  @input="liveSettings.setDanmakuOpacity(($event.target as HTMLInputElement).valueAsNumber)"
                >
                <span class="setting-slider-value">{{ Math.round(liveSettings.danmakuOpacity * 100) }}%</span>
              </div>

              <div class="setting-slider-row">
                <label>字号</label>
                <input
                  type="range"
                  :min="danmakuFontSizeRange.min"
                  :max="danmakuFontSizeRange.max"
                  step="1"
                  :value="liveSettings.danmakuFontSize"
                  @input="liveSettings.setDanmakuFontSize(($event.target as HTMLInputElement).valueAsNumber)"
                >
                <span class="setting-slider-value">{{ liveSettings.danmakuFontSize }}px</span>
              </div>

              <div class="setting-slider-row">
                <label>滚动速度</label>
                <input
                  type="range"
                  :min="danmakuSpeedRange.min"
                  :max="danmakuSpeedRange.max"
                  step="1"
                  :value="liveSettings.danmakuSpeed"
                  @input="liveSettings.setDanmakuSpeed(($event.target as HTMLInputElement).valueAsNumber)"
                >
                <span class="setting-slider-value">{{ liveSettings.danmakuSpeed }}s</span>
              </div>

              <p v-if="danmakuError" class="setting-hint error">{{ danmakuError }}</p>
              <p v-else class="setting-hint">滚动速度为弹幕滚过画面的秒数，越小越快。</p>
            </section>

            <section v-if="streams.length" class="setting-group">
              <h3>画质 / 线路</h3>
              <div class="setting-options">
                <button
                  v-for="option in qualityOptions"
                  :key="option.quality"
                  class="setting-chip"
                  :class="{ active: activeQuality === option.quality }"
                  @click="selectQuality(option)"
                >
                  {{ option.quality }}
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
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import Hls from 'hls.js'
import flvjs from 'flv.js'
import { useLiveStore } from '../stores/live'
import { useLiveShieldStore } from '../stores/liveShield'
import {
  DANMAKU_AREA_OPTIONS,
  DANMAKU_FONT_SIZE_RANGE,
  DANMAKU_OPACITY_RANGE,
  DANMAKU_SPEED_RANGE,
  pickQualityIndex,
  useLiveSettingsStore,
} from '../stores/liveSettings'
import { addLiveStats } from '../api/liveStats'
import { addLiveHistory } from '../api/liveHistory'
import { getLiveSuperChatHistory } from '../api/live'
import { startLiveDanmaku, stopLiveDanmaku, type DanmakuMessage } from '../api/liveDanmaku'
import type { LiveQuality, LiveRoomDetail, LiveStream } from '../api/live'
import { createLoadTrace, type LoadTrace } from '../utils/diagnostics'

type PlayerStatus = 'loading' | 'playing' | 'error' | 'offline' | 'reconnecting'

const route = useRoute()
const router = useRouter()
const store = useLiveStore()
const liveSettings = useLiveSettingsStore()
const shield = useLiveShieldStore()
/** 桌面端全屏必须走 Tauri 窗口 API: WebView2 里对 DOM 调 requestFullscreen 不可靠
 *  (用户实测完全进不去)。浏览器预览时退回 DOM 全屏。 */
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
const appWindow = isTauri ? getCurrentWindow() : null
let unlistenWindowResize: (() => void) | undefined

const videoRef = ref<HTMLVideoElement | null>(null)
const detail = ref<LiveRoomDetail | null>(null)
const streams = ref<LiveStream[]>([])
/** 平台认可的全部清晰度(B 站登录后档位更多); 其它平台为空, 回退到按实际线路归并。 */
const qualities = ref<LiveQuality[]>([])
const activeIndex = ref(0)
const status = ref<PlayerStatus>('loading')
const errorMsg = ref('')
const muted = ref(true)

let hls: Hls | null = null
let flv: flvjs.Player | null = null
/** 本次进房的加载诊断采样；首帧、卸线、失败都会汇总成一行日志。 */
let liveTrace: LoadTrace | null = null
/** 本轮播放已试过的线路下标(容灾切换用), 每次 load() 重置。 */
const triedIndexes = new Set<number>()

/** 断流自愈: 指数退避 + 次数上限, 超过才切下一条线路。 */
const MAX_RECONNECT = 3
const reconnectCount = ref(0)
let lastReconnectTimer: number | undefined

/** FLV stalled 检测: currentTime 停滞超 3s 视为断流。 */
let flvStallTimer: number | undefined
let lastMediaTime = 0

/** 观看时长上报：播放中用固定间隔累加，离开页面/暂停时补一次。 */
let watchTimer: number | undefined
let watchTickAt = 0
let sessionCounted = false

type DanmakuItem = DanmakuMessage & { id: number; track: number }
/** 轨道数按「显示区域高度 ÷ 行高」动态计算, 默认上限 8: 全屏时行数多,
 *  小窗时自动减少, 避免弹幕落在被裁掉的轨道上而"看不见"。 */
const DANMAKU_MAX_LANES = 8
const danmakuLaneCount = ref(6)
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
const danmakuAreaOptions = DANMAKU_AREA_OPTIONS
const danmakuOpacityRange = DANMAKU_OPACITY_RANGE
const danmakuFontSizeRange = DANMAKU_FONT_SIZE_RANGE
const danmakuSpeedRange = DANMAKU_SPEED_RANGE
let danmakuRunning = false
const danmakuTimers = new Set<number>()

/** 全屏的对象是「播放器 + 信息栏」整体, 而不是只有画面:
 *  只把画面全屏会把右侧聊天/设置挡在外面, 导致全屏时设置无法打开。 */
const roomBodyRef = ref<HTMLElement | null>(null)
const playerShellRef = ref<HTMLElement | null>(null)
let playerShellObserver: ResizeObserver | null = null
const paused = ref(false)
const fitMode = ref<'contain' | 'cover' | 'fill'>('contain')
const rotate = ref(0)
const mirrored = ref(false)
const sideTab = ref<'chat' | 'follow' | 'settings'>('chat')
/** 底栏画质按钮弹出的清晰度列表。 */
const qualitySheetOpen = ref(false)
/** 全屏时侧栏默认隐藏(只留画面+弹幕), 点底栏齿轮/聊天按钮时以浮层弹出。 */
const isFullscreen = ref(false)
const fsPanel = ref<'chat' | 'follow' | 'settings' | null>(null)

function openSideTab(tab: 'chat' | 'follow' | 'settings') {
  sideTab.value = tab
  if (isFullscreen.value) fsPanel.value = tab
}

function closeFsPanel() {
  fsPanel.value = null
}

function syncFullscreenState() {
  const active = isTauri
    ? isFullscreen.value
    : Boolean(document.fullscreenElement)
  setFullscreenState(active)
}

/** 全屏 = 整个窗口全屏 + 隐藏标题栏/导航/侧栏, 只留画面和弹幕, 与 Simple Live 一致。 */
function setFullscreenState(active: boolean) {
  isFullscreen.value = active
  if (!active) fsPanel.value = null
  if (typeof document !== 'undefined') {
    document.body.classList.toggle('live-fullscreen', active)
  }
}

async function toggleFullscreen() {
  // 桌面端: 窗口级全屏(WebView2 的 DOM 全屏不可靠)。
  if (appWindow) {
    const next = !isFullscreen.value
    try {
      await appWindow.setFullscreen(next)
      setFullscreenState(next)
    } catch {
      // 窗口全屏失败时退回 DOM 全屏, 至少保证按钮有反馈。
    }
    return
  }
  // 浏览器预览: DOM 全屏。
  const body = roomBodyRef.value
  if (!body) return
  if (document.fullscreenElement) {
    await document.exitFullscreen().catch(() => undefined)
    return
  }
  await body.requestFullscreen().catch(() => undefined)
}

/** 全屏时按 Esc 退出(与常见桌面播放器一致)。 */
function onFullscreenEscape(event: KeyboardEvent) {
  if (event.key !== 'Escape' || !isFullscreen.value) return
  event.preventDefault()
  if (appWindow) {
    void appWindow.setFullscreen(false).catch(() => undefined)
  }
  setFullscreenState(false)
}
const watchedSeconds = ref(0)
const copied = ref(false)
let watchDisplayTimer: number | undefined

/** ── 开播时长: 有 live_start_time 的平台(B站/斗鱼)实时累计, 每 30 秒一跳 ── */
const liveUptimeText = ref('')
let liveUptimeTimer: number | undefined

/** ── SC 卡片横幅: 弹幕实时推送 + 进房拉取展示期内的历史, 剩余秒数递减 ── */
interface SuperChatCard {
  id: number
  user: string
  avatar: string
  message: string
  price: number
  /** 剩余展示秒数。 */
  remain: number
}
const superchats = ref<SuperChatCard[]>([])
let superchatSeq = 0
let superchatSeen = new Set<string>()

/** SC 金额分档配色(参考 B 站: 蓝→黄→红)。 */
function scAccent(price: number) {
  if (price >= 500) return '#c04851'
  if (price >= 100) return '#d0762c'
  if (price >= 50) return '#b8962e'
  return '#1e7fbf'
}

function pushSuperChat(card: Omit<SuperChatCard, 'id' | 'remain'>, remain: number) {
  // 同一发送者+内容的 SC 去重(历史与实时推送可能重叠)。
  const key = `${card.user}|${card.message}`
  if (superchatSeen.has(key)) return
  superchatSeen.add(key)
  superchats.value.push({ ...card, id: ++superchatSeq, remain: Math.max(1, Math.round(remain)) })
}

function dismissSuperchat(id: number) {
  superchats.value = superchats.value.filter((item) => item.id !== id)
}

/** 每秒一跳: 开播时长累计 + SC 剩余秒数递减。 */
function onSecondTick() {
  // 开播时长
  const start = detail.value?.liveStartTime ?? 0
  if (start > 0 && detail.value?.isLive) {
    liveUptimeText.value = formatClock(Math.max(0, Math.floor(Date.now() / 1000) - start))
  } else {
    liveUptimeText.value = ''
  }
  // SC 到期移除
  if (superchats.value.length) {
    superchats.value = superchats.value
      .map((item) => ({ ...item, remain: item.remain - 1 }))
      .filter((item) => item.remain > 0)
  }
}

function formatClock(total: number) {
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  const pad = (value: number) => String(value).padStart(2, '0')
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`
}

/** 进房时读取展示期内的 SC(B 站), 剩余时间按 end_time 换算。 */
async function loadSuperchatHistory() {
  if (platform !== 'bilibili') return
  try {
    const list = await getLiveSuperChatHistory(platform, roomId)
    const now = Math.floor(Date.now() / 1000)
    for (const item of list) {
      const remain = item.endTime - now
      if (remain > 0 && item.message) {
        pushSuperChat(
          {
            user: item.user,
            avatar: item.avatar,
            message: item.message,
            price: item.price,
          },
          remain,
        )
      }
    }
  } catch {
    // SC 历史拉不到不影响观看。
  }
}

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

/** 按播放器当前高度与显示区域重算可用轨道数。 */
function updateDanmakuLanes() {
  const height = playerShellRef.value?.clientHeight ?? 0
  if (!height) return
  const area = Math.max(1, Math.min(100, liveSettings.danmakuArea))
  const laneHeight = Math.max(1, danmakuLaneHeight.value)
  danmakuLaneCount.value = Math.max(
    1,
    Math.min(DANMAKU_MAX_LANES, Math.floor((height * area) / 100 / laneHeight)),
  )
}

const currentQuality = computed(() => streams.value[activeIndex.value]?.quality || '默认')

type QualityOption = { quality: string; qn?: number; index?: number }

/**
 * 清晰度列表:
 * - B 站用接口下发的 `accept_qn`(登录后档位更多), 选中后按 qn 重新取流;
 * - 其它平台的 streams 本身覆盖全部档位, 按画质归并(容灾时自动换同档线路)。
 */
const qualityOptions = computed<QualityOption[]>(() => {
  if (qualities.value.length) {
    return qualities.value.map((item) => ({ quality: item.name, qn: item.qn }))
  }
  const seen = new Map<string, number>()
  streams.value.forEach((stream, index) => {
    if (!seen.has(stream.quality)) seen.set(stream.quality, index)
  })
  return [...seen.entries()].map(([quality, index]) => ({ quality, index }))
})

/** 只有 B 站需要登录 Cookie; 其它平台忽略。 */
const bilibiliCookie = computed(() =>
  platform === 'bilibili' ? liveSettings.bilibiliCookie.trim() : '',
)

const activeQuality = computed(() => streams.value[activeIndex.value]?.quality || '')

/** 未登录时接口只给超清: 弹窗里提示去设置里填 Cookie。 */
const showLoginQualityHint = computed(
  () => platform === 'bilibili' && !bilibiliCookie.value && qualities.value.length > 0,
)

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
      // 连接中断/重连提示只属于弹幕面板, 不能写进播放器错误(播放器还在正常出画面)。
      danmakuError.value = message.content
      continue
    }
    // 弹幕屏蔽: 命中屏蔽词的普通弹幕直接丢弃(付费 SC 不屏蔽)。
    if (message.kind === 'chat' && shield.shouldBlock(message.content, message.user)) {
      continue
    }
    // 收到真实弹幕后把之前的连接错误收起来。
    danmakuError.value = ''
    const item: DanmakuItem = {
      ...message,
      id: ++danmakuSeq,
      track: danmakuLane++ % Math.max(1, danmakuLaneCount.value),
    }
    // SC 同步进播放器上方的卡片横幅, 按 duration 倒计时。
    if (message.kind === 'superchat') {
      pushSuperChat(
        {
          user: message.user,
          avatar: message.avatar || '',
          message: message.content,
          price: message.price || 0,
        },
        message.duration || 60,
      )
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
  liveSettings.setDanmakuOn(!liveSettings.danmakuOn)
}

/** 设置面板的「弹幕开/关」: 与播放器底栏按钮共用同一份持久化开关。 */
function setDanmakuEnabled(value: boolean) {
  if (liveSettings.danmakuOn !== value) liveSettings.setDanmakuOn(value)
}

// 开关变化时同步连接状态: 关闭立即断开, 打开重新连接(后端会顶掉旧连接)。
watch(
  () => liveSettings.danmakuOn,
  (enabled) => {
    if (enabled) {
      void startDanmaku()
      return
    }
    stopDanmaku()
    floatingItems.value = []
  },
)

// 显示区域/字号变化会改变可用轨道数; 全屏切换则由播放器尺寸的 ResizeObserver 触发。
watch(
  [() => liveSettings.danmakuArea, () => liveSettings.danmakuFontSize],
  () => void nextTick(updateDanmakuLanes),
)

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
  stopFlvStallMonitor()
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

/* ---------- 断流自愈 ---------- */

function clearReconnectTimer() {
  if (lastReconnectTimer !== undefined) {
    clearTimeout(lastReconnectTimer)
    lastReconnectTimer = undefined
  }
}

/** 手动切换到下一条线路（overlay 按钮）。 */
function manualSwitchLine() {
  clearReconnectTimer()
  reconnectCount.value = MAX_RECONNECT // 让 tryNextStream 直接尝试下一条
  tryNextStream('手动切换线路')
}

/** HLS 致命错误自愈：NETWORK_ERROR → startLoad, MEDIA_ERROR → recoverMediaError。
 *  指数退避 1s/2s/4s，超过上限才切线路。 */
function hlsRecover(instance: Hls, data: { type: string; details: string }) {
  // 已经在自愈中且还没到上限, 不重复累加
  if (status.value === 'reconnecting') return
  liveTrace?.stage('retry')

  const delay = Math.min(1000 * 2 ** reconnectCount.value, 4000)
  reconnectCount.value++
  status.value = 'reconnecting'
  errorMsg.value = ''

  clearReconnectTimer()
  lastReconnectTimer = window.setTimeout(() => {
    if (reconnectCount.value > MAX_RECONNECT) {
      instance.destroy()
      hls = null
      tryNextStream(`自愈失败（${data.type}: ${data.details}）`)
      return
    }
    try {
      if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
        instance.startLoad()
      } else if (
        data.type === Hls.ErrorTypes.MEDIA_ERROR
        && typeof (instance as unknown as { recoverMediaError?: () => boolean }).recoverMediaError === 'function'
      ) {
        ;(instance as unknown as { recoverMediaError: () => boolean }).recoverMediaError()
      } else {
        // 无法自愈的类型, 直接切线路
        instance.destroy()
        hls = null
        tryNextStream(`播放中断（${data.type}: ${data.details}）`)
        return
      }
      // 等待 1 秒看是否恢复
      const checkRecovery = window.setTimeout(() => {
        if (status.value === 'reconnecting' && hls === instance) {
          // 还在 reconnecting 说明没恢复, 让下一次 ERROR 事件继续自愈
          status.value = 'playing' // 先设回 playing, 让 hls.js 自己恢复或触发新的 fatal
        }
      }, 1500)
      // 存一下以便清理（用 lastReconnectTimer 不够, 但这里简化）
      void checkRecovery
    } catch {
      instance.destroy()
      hls = null
      tryNextStream(`自愈异常（${data.type}: ${data.details}）`)
    }
  }, delay)
}

/** FLV 致命错误自愈：指数退避重建播放器。 */
function flvRecover(reason: string, url: string, video: HTMLVideoElement) {
  if (status.value === 'reconnecting') return
  liveTrace?.stage('retry')
  stopFlvStallMonitor()

  const delay = Math.min(1000 * 2 ** reconnectCount.value, 4000)
  reconnectCount.value++
  status.value = 'reconnecting'
  errorMsg.value = ''

  clearReconnectTimer()
  lastReconnectTimer = window.setTimeout(() => {
    if (reconnectCount.value > MAX_RECONNECT) {
      tryNextStream(reason)
      return
    }
    // 销毁旧播放器, 重建新的
    if (flv) {
      flv.pause()
      flv.unload()
      flv.detachMediaElement()
      flv.destroy()
      flv = null
    }
    const player = flvjs.createPlayer(
      { type: 'flv', url, isLive: true, hasAudio: true, hasVideo: true },
      { enableStashBuffer: false, autoCleanupSourceBuffer: true, lazyLoad: false },
    )
    flv = player
    player.on(flvjs.Events.ERROR, (t: string, d: string) => flvRecover(`播放中断（${t}: ${d}）`, url, video))
    player.attachMediaElement(video)
    player.load()
    player.play()
    startFlvStallMonitor(url, video)
  }, delay)
}

/** FLV stalled 检测: currentTime 停滞超 3s → 重建播放器。 */
function startFlvStallMonitor(url: string, video: HTMLVideoElement) {
  stopFlvStallMonitor()
  lastMediaTime = video.currentTime
  flvStallTimer = window.setInterval(() => {
    if (video.currentTime > lastMediaTime + 0.1) {
      lastMediaTime = video.currentTime
      return // 正常推进
    }
    // currentTime 停滞
    if (status.value !== 'playing') return // 已经在处理了
    flvRecover('播放卡顿（currentTime 停滞）', url, video)
  }, 1000)
}

function stopFlvStallMonitor() {
  if (flvStallTimer !== undefined) {
    clearInterval(flvStallTimer)
    flvStallTimer = undefined
  }
}

function markBuffering() {
  if (status.value === 'playing') status.value = 'loading'
}

function onPlaying() {
  status.value = 'playing'
  paused.value = false
  startWatchClock()
  liveTrace?.stage('firstFrame')
  liveTrace?.finish('firstFrame', {
    qualityIndex: activeIndex.value + 1,
    lineCount: streams.value.length,
  })
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
  liveTrace = createLoadTrace('live-load', { platform, trigger: 'initial' })
  // 新一轮加载: 容灾重试与 SC 去重都从头计。
  triedIndexes.clear()
  superchatSeen = new Set()
  superchats.value = []
  reconnectCount.value = 0
  clearReconnectTimer()
  // 先对齐平台, 再用 store 里的取流/详情方法(它们读的是 store.platform)。
  if (store.platform !== platform) store.platform = platform
  try {
    const [roomDetail, streamInfo] = await Promise.all([
      store.fetchRoomDetail(roomId),
      store.fetchStreamUrls(roomId, { cookie: bilibiliCookie.value || undefined }),
    ])
    liveTrace?.stage('detail')
    detail.value = roomDetail
    // 进房即记一次观看历史(带封面/头像快照, 未开播也记, 与 simple_live 一致)。
    void addLiveHistory({
      platform,
      roomId,
      roomName: roomDetail.title,
      anchorName: roomDetail.userName,
      cover: roomDetail.cover,
      avatar: roomDetail.avatar,
    }).catch(() => undefined)
    // 进房读取展示期内的 SC(B 站), 与弹幕实时推送合并展示。
    void loadSuperchatHistory()
    /**
     * 只有部分平台的流能在 WebView 里直接播:
     * - 虎牙/哔哩哔哩的 flv 校验 Referer(浏览器禁止脚本改这个头) → 只用 hls;
     * - 斗鱼只给 flv, 但其 CDN 不校验 Referer 且回了 `Access-Control-Allow-Origin: *`,
     *   交给 flv.js 播; 抖音的 flv 地址自带鉴权参数, 同样不校验 Referer。
     */
    qualities.value = streamInfo.qualities ?? []
    streams.value = playableStreams(streamInfo.streams)
    if (!streamInfo.isLive || !streams.value.length) {
      status.value = 'offline'
      liveTrace?.finish('offline', { lineCount: streams.value.length })
      return
    }
    // B 站首次请求只会拿到接口给的默认档位; 已登录时按偏好再取一次目标清晰度。
    // 虎牙等按 qn 选档的平台同理(未登录的 B 站无论传什么 qn 都只会下发超清)。
    const canPickQuality = platform !== 'bilibili' || Boolean(bilibiliCookie.value)
    const target = preferredQuality()
    if (canPickQuality && target && streams.value[0]?.quality !== target.name) {
      await fetchQualityStreams(target.qn, false)
    }
    activeIndex.value = qualities.value.length
      ? 0
      : pickQualityIndex(streams.value.length, liveSettings.qualityLevel)
    qualitySheetOpen.value = false
    await nextTick()
    liveTrace?.stage('quality')
    liveTrace?.stage('player')
    playCurrent()
    void startDanmaku()
  } catch (e) {
    status.value = 'error'
    errorMsg.value = (e as Error).message || '加载直播间失败'
    liveTrace?.fail('detail', { lineCount: 0 })
  }
}

/** 只保留 WebView 能直接播放的封装。 */
function playableStreams(list: LiveStream[]) {
  return list.filter(
    (item) =>
      item.format === 'hls' ||
      ((platform === 'douyu' || platform === 'douyin') && item.format === 'flv'),
  )
}

/** 按「最高/中等/最低」偏好从平台下发的清晰度里选中一档。 */
function preferredQuality(): LiveQuality | null {
  if (!qualities.value.length) return null
  return qualities.value[pickQualityIndex(qualities.value.length, liveSettings.qualityLevel)] ?? null
}

/** 按 qn 重新取流; 拿不到地址时保留当前线路。 */
async function fetchQualityStreams(qn: number, autoplay = true): Promise<boolean> {
  try {
    const info = await store.fetchStreamUrls(roomId, {
      cookie: bilibiliCookie.value || undefined,
      qn,
    })
    const list = playableStreams(info.streams)
    if (!list.length) return false
    qualities.value = info.qualities ?? qualities.value
    streams.value = list
    activeIndex.value = 0
    triedIndexes.clear()
    if (autoplay) playCurrent()
    return true
  } catch {
    return false
  }
}

/** 清晰度弹窗里选中一档: B 站按 qn 重新取流, 其它平台直接切已有线路。 */
async function selectQuality(option: QualityOption) {
  qualitySheetOpen.value = false
  if (option.qn === undefined) {
    if (option.index !== undefined) switchStream(option.index)
    return
  }
  if (option.quality === activeQuality.value) return
  await fetchQualityStreams(option.qn)
}

/** 播放失败时的线路容灾: 优先同画质下一条未试过的线路, 全试完才报错。 */
function tryNextStream(reason: string) {
  liveTrace?.stage('switchLine')
  const current = streams.value[activeIndex.value]
  const candidates = streams.value
    .map((stream, index) => ({ stream, index }))
    .filter(
      ({ stream, index }) =>
        !triedIndexes.has(index) &&
        (current ? stream.quality === current.quality : true) &&
        stream.url !== current?.url,
    )
  const fallbackSameQuality = candidates[0]
  if (fallbackSameQuality) {
    activeIndex.value = fallbackSameQuality.index
    playCurrent()
    return
  }
  // 同画质没有可用线路, 再尝试其它画质的线路(总比黑屏好)。
  const fallbackAny = streams.value.findIndex(
    (_stream, index) => !triedIndexes.has(index) && index !== activeIndex.value,
  )
  if (fallbackAny >= 0) {
    activeIndex.value = fallbackAny
    playCurrent()
    return
  }
  status.value = 'error'
  errorMsg.value = reason
}

function playCurrent() {
  const stream = streams.value[activeIndex.value]
  const video = videoRef.value
  if (!stream?.url || !video) return
  destroyPlayer()
  triedIndexes.add(activeIndex.value)
  reconnectCount.value = 0
  clearReconnectTimer()
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
      // 非致命错误由 hls.js 内部处理（自动恢复）
      if (!data.fatal) return
      // 致命错误: 先自愈, 超过上限再切线路
      hlsRecover(instance, data)
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
    // FLV 致命错误: 自愈 1-2-4 秒退避, 上限 MAX_RECONNECT 次
    flvRecover(`播放中断（${type}: ${detail}）`, url, video)
  })
  player.attachMediaElement(video)
  player.load()
  player.play()
  // FLV stalled 检测: currentTime 停滞超 3s 重建播放器
  startFlvStallMonitor(url, video)
}

function switchStream(index: number) {
  if (index === activeIndex.value) return
  activeIndex.value = index
  playCurrent()
}

/** 打开/收起底栏的清晰度列表。 */
function toggleQualitySheet() {
  qualitySheetOpen.value = !qualitySheetOpen.value
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
  document.addEventListener('fullscreenchange', syncFullscreenState)
  window.addEventListener('keydown', onFullscreenEscape)
  if (appWindow) {
    void appWindow
      .onResized(async () => {
        const full = await appWindow.isFullscreen().catch(() => false)
        if (full !== isFullscreen.value) setFullscreenState(full)
      })
      .then((unlisten) => {
        unlistenWindowResize = unlisten
      })
  }
  void store.loadFollows()
  void load()
  // 开播时长 + SC 剩余秒数共用一个 1s tick。
  liveUptimeTimer = window.setInterval(onSecondTick, 1000)
  onSecondTick()
  void nextTick(() => {
    updateDanmakuLanes()
    const shell = playerShellRef.value
    if (shell && typeof ResizeObserver !== 'undefined') {
      playerShellObserver = new ResizeObserver(() => updateDanmakuLanes())
      playerShellObserver.observe(shell)
    }
  })
})

onBeforeUnmount(() => {
  document.removeEventListener('fullscreenchange', syncFullscreenState)
  window.removeEventListener('keydown', onFullscreenEscape)
  unlistenWindowResize?.()
  unlistenWindowResize = undefined
  if (appWindow && isFullscreen.value) {
    void appWindow.setFullscreen(false).catch(() => undefined)
  }
  setFullscreenState(false)
  playerShellObserver?.disconnect()
  playerShellObserver = null
  if (liveUptimeTimer !== undefined) {
    window.clearInterval(liveUptimeTimer)
    liveUptimeTimer = undefined
  }
  superchats.value = []
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

.room-uptime {
  padding: 4px 10px;
  border-radius: var(--radius-full);
  background: var(--color-bg-sunken);
  color: var(--color-text-tertiary);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}

/* ── SC 卡片横幅: 播放器上方, 到期自动移除 ── */
.superchat-layer {
  position: absolute;
  top: 10px;
  left: 10px;
  right: 10px;
  z-index: 3;
  display: flex;
  flex-direction: column;
  gap: 6px;
  pointer-events: none;
}

.sc-card {
  display: flex;
  align-items: stretch;
  gap: 8px;
  max-width: 460px;
  padding: 8px 10px;
  border-radius: var(--radius-md);
  background: var(--sc-accent, #1e7fbf);
  color: #fff;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
  pointer-events: auto;
}

.sc-avatar {
  width: 34px;
  height: 34px;
  border-radius: 50%;
  object-fit: cover;
  flex-shrink: 0;
  border: 2px solid rgba(255, 255, 255, 0.6);
}

.sc-avatar.placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: rgba(255, 255, 255, 0.25);
  font-size: 15px;
}

.sc-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.sc-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.sc-head strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sc-price {
  padding: 1px 7px;
  border-radius: var(--radius-full);
  background: rgba(255, 255, 255, 0.22);
  font-weight: 700;
  font-size: 11px;
  flex-shrink: 0;
}

.sc-remain {
  margin-left: auto;
  color: rgba(255, 255, 255, 0.85);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}

.sc-message {
  margin: 0;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  font-size: 12px;
  line-height: 1.4;
}

.sc-close {
  align-self: flex-start;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  color: rgba(255, 255, 255, 0.75);
  font-size: 14px;
  line-height: 1;
  flex-shrink: 0;
}

.sc-close:hover {
  background: rgba(255, 255, 255, 0.2);
  color: #fff;
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

/* 全屏: 整个窗口全屏, 隐藏标题栏/导航/房间头部/聊天栏, 只留画面+弹幕。
 * 与 Simple Live 一致; 设置由播放器底栏齿轮以浮层弹出。 */
.live-room-view.is-fullscreen {
  position: relative;
  padding: 0;
}

.live-room-view.is-fullscreen .room-head,
.live-room-view.is-fullscreen .room-actions {
  display: none;
}

.live-room-view.is-fullscreen .room-body {
  gap: 0;
}

.live-room-view.is-fullscreen .player-shell {
  border: none;
  border-radius: 0;
}

.live-room-view.is-fullscreen .room-side {
  display: none;
}

.live-room-view.is-fullscreen .room-side.fs-open {
  display: flex;
  position: absolute;
  top: 12px;
  right: 12px;
  bottom: 12px;
  width: min(360px, 46vw);
  z-index: 8;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.45);
}

.live-room-view.is-fullscreen .room-side .side-head,
.live-room-view.is-fullscreen .room-side .side-tabs {
  display: none;
}

.live-room-view.is-fullscreen .room-side .side-panel {
  padding-top: 38px;
}

.side-close {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: 50%;
  background: var(--color-bg-sunken);
  color: var(--color-text-secondary);
  font-size: 17px;
  line-height: 1;
}

.side-close:hover {
  color: var(--color-text);
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
  /* 铺满左栏剩余高度, 与右侧信息栏等高 —— 原先是固定 16:9 小画面,
   * 窗口高时下方留出大片空白, 和侧栏比例不协调。 */
  flex: 1;
  min-height: 0;
  overflow: hidden;
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-light);
  background: #000;
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

/* 清晰度列表: 与 Simple Live 一致, 底栏画质按钮弹出, 点击即切线路。 */
.quality-backdrop {
  position: absolute;
  inset: 0;
  z-index: 4;
}

.quality-sheet {
  position: absolute;
  right: 10px;
  bottom: 48px;
  z-index: 5;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 128px;
  max-height: min(320px, 60%);
  padding: 6px;
  overflow-y: auto;
  border-radius: var(--radius-md);
  background: rgba(22, 22, 24, 0.92);
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(10px);
}

.quality-sheet-head {
  padding: 4px 10px 6px;
  color: rgba(255, 255, 255, 0.5);
  font-size: 11px;
  letter-spacing: 0.04em;
}

.quality-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  color: rgba(255, 255, 255, 0.86);
  font-size: 12.5px;
  text-align: left;
  transition: background var(--duration-fast) var(--ease-out);
}

.quality-item:hover {
  background: rgba(255, 255, 255, 0.12);
}

.quality-item.active {
  background: var(--color-primary);
  color: #fff;
  font-weight: 600;
}

.quality-check {
  font-size: 12px;
}

.quality-hint {
  margin: 0;
  padding: 8px 14px 10px;
  border-top: 1px solid var(--color-divider);
  color: var(--color-text-tertiary);
  font-size: 11px;
  line-height: 1.5;
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

.setting-options-inline {
  flex: 1;
  margin-top: 0 !important;
}

.setting-slider-row {
  display: grid;
  grid-template-columns: 58px 1fr 42px;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
  color: var(--color-text-secondary);
  font-size: 12px;
}

.setting-slider-row input[type='range'] {
  width: 100%;
  min-width: 0;
  accent-color: var(--color-primary);
}

.setting-slider-value {
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  text-align: right;
}

.setting-hint {
  margin: 8px 0 0;
  color: var(--color-text-tertiary);
  font-size: 11px;
  line-height: 1.6;
}

.setting-hint.error {
  color: var(--color-danger, #c0392b);
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
    flex: 0 0 auto;
    aspect-ratio: 16 / 9;
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
  }
}
</style>
