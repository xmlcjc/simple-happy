<template>
  <div
    class="app-shell"
    :class="{ 'has-custom-background': showCustomBackground }"
    :style="appShellStyle"
  >
    <div
      v-if="showCustomBackground"
      class="app-custom-background"
      aria-hidden="true"
      :style="customBackgroundStyle"
    ></div>
    <TitleBar :surface="titlebarSurface" />
    <div class="app-body">
      <AppTopBar v-if="showHeader" />
      <main class="app-main" :class="{ 'without-header': !showHeader }">
        <router-view />
      </main>
    </div>
    <SettingsDrawer v-model="appStore.showSettingsDrawer" />
    <SourceManager v-model="appStore.showSourceManager" />
    <WebdavManager v-model="appStore.showWebdavManager" />
    <LogViewerModal v-model="appStore.showLogViewer" />
    <ConfirmDialog />
    <ContextMenu />

    <!-- 全局音乐播放器（单例）：任何页面都能看到。阅读页和直播全屏时隐藏。 -->
    <MusicPlayerBar
      v-if="current && route.name !== 'reader' && !isLiveFullscreen"
      class="global-player-bar"
      :current="current"
      :playing="playing"
      :loading="loading"
      :current-time="currentTime"
      :duration="duration"
      :volume="volume"
      :error-text="errorText"
      :lyric-open="lyricOpen"
      :play-mode="playerMode"
      :playback-rate="playbackRate"
      :queue-count="queue.length"
      :queue-open="queueOpen"
      :quality-label="qualityLabel"
      :preferred-quality="preferredQuality"
      :quality-order="qualityOrder"
      :swapped-from="swappedFrom"
      :sleep-mode="sleepMode"
      :sleep-remaining="sleepRemaining"
      @toggle="toggle"
      @next="next"
      @prev="prev"
      @seek="seek"
      @volume="setVolume"
      @toggle-lyric="lyricOpen = !lyricOpen; if (lyricOpen) queueOpen = false"
      @toggle-queue="queueOpen = !queueOpen; if (queueOpen) lyricOpen = false"
      @cycle-mode="cycleMode"
      @cycle-rate="cycleRate"
      @set-quality="changeQuality"
      @set-quality-order="setQualityOrder"
      @set-sleep="player.setSleepCountdown"
      @sleep-end="player.setSleepEndOfTrack"
      @cancel-sleep="player.cancelSleep"
    />

    <!-- 全局歌词面板 -->
    <MusicLyricPanel
      v-if="lyricOpen && current && route.name !== 'reader' && !isLiveFullscreen"
      class="global-lyric-panel"
      :lines="lyricLines"
      :current-index="lyricIndex"
      :loading="lyricLoading"
      :error="lyricError"
      :offset-ms="settings.lyricOffsetMs"
      :font-size-px="lyricFontPx"
      :font-label="lyricFontLabel"
      @close="lyricOpen = false"
      @seek="seek"
      @shift-offset="shiftLyricOffset"
      @reset-offset="settings.setLyricOffsetMs(0)"
      @cycle-font-size="cycleLyricFontSize"
    />

    <!-- 全局队列面板 -->
    <MusicQueuePanel
      v-if="queueOpen && route.name !== 'reader' && !isLiveFullscreen"
      class="global-queue-panel"
      :items="queue"
      :current-index="queueIndex"
      @close="queueOpen = false"
      @jump="jumpTo"
      @remove="removeAt"
      @clear="clearQueue"
    />

    <!-- Toast notifications -->
    <div class="toast-container">
      <TransitionGroup name="slide-up">
        <div
          v-for="toast in appStore.toasts"
          :key="toast.id"
          class="toast"
          :class="toast.type"
          role="status"
          title="点击关闭"
          @click="appStore.dismissToast(toast.id)"
        >
          {{ toast.message }}
        </div>
      </TransitionGroup>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { listen } from '@tauri-apps/api/event'
import { useAppStore } from './stores/app'
import { useReaderStore } from './stores/reader'
import { useMusicPlayer } from './composables/useMusicPlayer'
import { useMusicLyric } from './composables/useMusicLyric'
import { nextPlaybackRate, LYRIC_FONT_SIZE_OPTIONS, useMusicSettingsStore, type MusicQuality, type MusicQualityOrder } from './stores/musicSettings'
import AppTopBar from './components/AppTopBar.vue'
import SettingsDrawer from './components/SettingsDrawer.vue'
import SourceManager from './components/SourceManager.vue'
import WebdavManager from './components/WebdavManager.vue'
import LogViewerModal from './components/LogViewerModal.vue'
import ConfirmDialog from './components/ConfirmDialog.vue'
import ContextMenu from './components/ContextMenu.vue'
import TitleBar from './components/TitleBar.vue'
import MusicPlayerBar from './components/music/MusicPlayerBar.vue'
import MusicLyricPanel from './components/music/MusicLyricPanel.vue'
import MusicQueuePanel from './components/music/MusicQueuePanel.vue'
import { resolveWindowClose } from './utils/windowClose'
import { resolveShellBackgroundImage } from './utils/readerColor'

const route = useRoute()
const appStore = useAppStore()
const readerStore = useReaderStore()
const player = useMusicPlayer()
// 解构出 template 要用的响应式属性（Vue 自动 unwrap）
const { current, playing, loading, currentTime, duration, volume, errorText, queue, index: queueIndex, effectiveQuality, swappedFrom, sleepMode, sleepRemaining, toggle, next, prev, seek, setVolume, reload, jumpTo, removeAt, clearQueue } = player
const qualityLabel = computed(() => effectiveQuality.value || '')
// 设置 store（播放器需要的几项）
const settings = useMusicSettingsStore()
const playerMode = computed(() => settings.playMode)
const playbackRate = computed(() => settings.playbackRate)
const preferredQuality = computed(() => settings.preferredQuality)
const qualityOrder = computed(() => settings.qualityOrder)
const cycleMode = () => settings.cyclePlayMode()
const cycleRate = () => settings.setPlaybackRate(nextPlaybackRate(settings.playbackRate))
const setQualityOrder = (order: MusicQualityOrder) => settings.setQualityOrder(order)

// 歌词/队列全局面板状态
const lyricOpen = ref(false)
const queueOpen = ref(false)
const { lines: lyricLines, currentIndex: lyricIndex, loading: lyricLoading, error: lyricError } = useMusicLyric(current, currentTime)

const lyricFontOption = computed(
  () => LYRIC_FONT_SIZE_OPTIONS.find((o) => o.value === settings.lyricFontSize) ?? LYRIC_FONT_SIZE_OPTIONS[1],
)
const lyricFontPx = computed(() => lyricFontOption.value.px)
const lyricFontLabel = computed(() => lyricFontOption.value.label)

function shiftLyricOffset(deltaMs: number) {
  settings.setLyricOffsetMs(settings.lyricOffsetMs + deltaMs)
}
function cycleLyricFontSize() {
  const idx = LYRIC_FONT_SIZE_OPTIONS.findIndex((o) => o.value === settings.lyricFontSize)
  const next = LYRIC_FONT_SIZE_OPTIONS[(idx + 1) % LYRIC_FONT_SIZE_OPTIONS.length]
  settings.setLyricFontSize(next.value)
}
function changeQuality(quality: MusicQuality) {
  settings.setPreferredQuality(quality)
  if (current.value) reload()
}

// 直播全屏检测
const isLiveFullscreen = ref(false)
onMounted(() => {
  const check = () => { isLiveFullscreen.value = typeof document !== 'undefined' && document.body.classList.contains('live-fullscreen') }
  check()
  if (typeof MutationObserver !== 'undefined') {
    new MutationObserver(check).observe(document.body, { attributes: true, attributeFilter: ['class'] })
  }
})

const showHeader = computed(() => route.name !== 'reader')
const titlebarSurface = computed<'settings' | 'reader-panel' | null>(() => {
  if (appStore.showSettingsDrawer) return 'settings'
  if (appStore.showSourceManager || appStore.showWebdavManager || appStore.showLogViewer) return 'settings'
  if (route.name === 'reader' && readerStore.activePanel) return 'reader-panel'
  return null
})
const showCustomBackground = computed(() => Boolean(shellBackgroundImage.value))
const shellBackgroundImage = computed(() => {
  const onReader = route.name === 'reader'
  // 不按页面清空参数: 由 resolveShellBackgroundImage 自己按 onReader 分流。
  // 早先在这里把阅读页专属参数置空, 配合函数内部的阅读页优先级, 会让
  // applyBackgroundToReader=false 时连桌面背景都判成空。
  return resolveShellBackgroundImage({
    customBackgroundColor: readerStore.readerBackgroundColor,
    readerBackgroundImage: readerStore.config.readerBackgroundImage,
    backgroundImage: readerStore.config.backgroundImage,
    applyBackgroundToReader: readerStore.config.applyBackgroundToReader,
  }, onReader)
})
const appShellStyle = computed(() => ({
  background: route.name === 'reader'
    ? readerStore.readerTheme.body
    : 'var(--color-bg)',
}))
const customBackgroundStyle = computed(() => {
  // 透明度要跟着**实际铺出去的那张图**走: 铺的是阅读页专属图就用它的透明度,
  // 否则用桌面图自己的。早先只看 readerBackgroundImage 是否存在, 于是在桌面页
  // (铺的是桌面图)也会误用阅读页专属图的透明度。
  const usingReaderImage = route.name === 'reader'
    && Boolean(readerStore.config.readerBackgroundImage)
  return {
    // 图铺在外壳最底层(标题栏也在其中), 因此标题栏必须透明才能透出背景。
    backgroundImage: shellBackgroundImage.value ? `url(${shellBackgroundImage.value})` : undefined,
    opacity: usingReaderImage
      ? readerStore.config.readerImageOpacity
      : readerStore.config.backgroundOpacity,
  }
})

const stopBackgroundClassSync = watch(showCustomBackground, (active) => {
  if (typeof document === 'undefined') return
  document.body.classList.toggle('custom-background-active', active)
}, { immediate: true })

onMounted(() => {
  void appStore.applyBossKey().catch(() => undefined)
  void appStore.applyNetworkProxy().catch(() => undefined)
})

let closeUnlisten: (() => void) | undefined
let handlingCloseRequest = false

/** 关闭窗口前的收尾: 把阅读位置落到书架。
 *
 *  legado 在 `ReadBookActivity.onPause` 里必然执行 `ReadBook.saveRead()`, 关闭/切走
 *  的那一刻位置就已写库。本项目的等价时机只有这里: `destroy()` 会立即销毁
 *  WebView, `beforeunload` 与 Vue 的 `onUnmounted` 都不保证执行, 异步 IPC 也会被
 *  终止。所以必须在销毁窗口**之前**主动 flush 一次。
 *
 *  阅读页用 keepalive 版本(一次 IPC 往返即返回, 不等 WebDAV 上传); 其它页面用
 *  普通版本 —— 阅读器状态在离开阅读页后仍保留在 store 里, 从这里补一次能救回
 *  "边界情况下路由离开钩子没跑完"的进度。两者都受 `resolveWindowClose` 的
 *  1.5s 上限约束, 不会把关窗拖成卡顿。 */
async function persistBeforeClose() {
  // 保存并清理播放器（先落库再清空，保证下次启动可恢复）。
  player.shutdown()
  if (route.name === 'reader') {
    await readerStore.flushProgressToServerKeepalive(true)
    return
  }
  await readerStore.flushProgressToServer(true)
}

onMounted(() => {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return
  listen<unknown>('close-requested', async () => {
    if (handlingCloseRequest) return
    handlingCloseRequest = true
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const { invoke } = await import('@tauri-apps/api/core')
      await resolveWindowClose(
        getCurrentWindow(),
        appStore.closeToTray,
        persistBeforeClose,
        // 回执让外壳的关窗看门狗退让。没有它, 选择"关闭到托盘"后从托盘重新
        // 打开窗口会撞上看门狗的宽限期, 窗口被当作"没人响应"直接销毁。
        () => invoke('acknowledge_close_request'),
      )
    } finally {
      handlingCloseRequest = false
    }
  }).then((un) => {
    closeUnlisten = un
  })
})

onBeforeUnmount(() => {
  closeUnlisten?.()
  stopBackgroundClassSync()
  if (typeof document !== 'undefined') {
    document.body.classList.remove('custom-background-active')
  }
})
</script>

<style>
html,
body {
  height: var(--app-height, 100dvh);
  overflow: hidden;
}

.app-shell {
  height: var(--app-height, 100dvh);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  position: relative;
  transition: background var(--duration-normal);
}

.app-custom-background {
  position: absolute;
  inset: 0;
  z-index: 0;
  background-position: center;
  background-repeat: no-repeat;
  background-size: cover;
  pointer-events: none;
  transition: opacity var(--duration-normal);
}

.app-shell > .titlebar {
  position: relative;
  z-index: calc(var(--z-modal) + 20);
}

.app-shell > .app-body {
  position: relative;
  z-index: 1;
}

.app-shell.has-custom-background .app-topbar {
  background: color-mix(in srgb, var(--color-bg-elevated) 74%, transparent);
  border-bottom-color: color-mix(in srgb, var(--color-border) 72%, transparent);
  backdrop-filter: blur(20px) saturate(125%);
  -webkit-backdrop-filter: blur(20px) saturate(125%);
}

.app-shell.has-custom-background .shelf-btn,
.app-shell.has-custom-background .book-card,
.app-shell.has-custom-background .batch-toolbar {
  background: color-mix(in srgb, var(--color-bg-elevated) 82%, transparent);
  border-color: color-mix(in srgb, var(--color-border) 76%, transparent);
}

.app-shell.has-custom-background .topbar-nav {
  background: color-mix(in srgb, var(--color-bg-elevated) 78%, transparent);
  border-color: color-mix(in srgb, var(--color-border) 74%, transparent);
  backdrop-filter: blur(18px) saturate(120%);
  -webkit-backdrop-filter: blur(18px) saturate(120%);
}

.app-shell.has-custom-background .reader-search-panel,
.app-shell.has-custom-background .tts-controls {
  backdrop-filter: blur(20px) saturate(120%);
  -webkit-backdrop-filter: blur(20px) saturate(120%);
  border-color: color-mix(in srgb, currentColor 10%, transparent);
}

.app-shell.has-custom-background :is(
  .stats-view,
  .rss-view,
  .rss-manage-view,
  .rss-article-view,
  .recent-view,
  .live-view,
  .live-room-view
) {
  background: transparent !important;
}

.app-shell.has-custom-background :is(
  .stat-card,
  .chart-card,
  .book-stats-card,
  .range-chip,
  .rss-toolbar,
  .article-list-panel,
  .article-content-panel,
  .article-page-card,
  .rss-manage-hero,
  .rss-manage-tools,
  .rss-sources,
  .rss-editor,
  .recent-search-input,
  .filter-chip,
  .recent-clear-btn,
  .refresh-btn,
  .group-chip,
  .area-chip,
  .quality-chip,
  .control-btn,
  .room-info
) {
  background: color-mix(in srgb, var(--color-bg-elevated) 82%, transparent) !important;
  border-color: color-mix(in srgb, var(--color-border) 76%, transparent) !important;
  backdrop-filter: blur(18px) saturate(120%);
  -webkit-backdrop-filter: blur(18px) saturate(120%);
}

body.custom-background-active {
  --glass-window-bg: color-mix(in srgb, var(--color-bg-elevated) 82%, transparent);
  --glass-inner-bg: color-mix(in srgb, var(--color-bg-elevated) 58%, transparent);
  --glass-border-color: color-mix(in srgb, var(--color-border) 72%, transparent);
}

body.custom-background-active :is(
  .source-modal,
  .login-preview-modal,
  .subscription-panel,
  .webdav-modal,
  .detail-modal,
  .cache-modal,
  .confirm-dialog,
  .modal-card,
  .modal-content
) {
  background: var(--glass-window-bg) !important;
  border-color: var(--glass-border-color) !important;
  backdrop-filter: blur(24px) saturate(120%);
  -webkit-backdrop-filter: blur(24px) saturate(120%);
}

body.custom-background-active :is(
  .reader-drawer,
  .reader-search-panel,
  .tts-controls,
  .selection-menu
) {
  border-color: color-mix(in srgb, currentColor 10%, transparent) !important;
  backdrop-filter: blur(24px) saturate(120%);
  -webkit-backdrop-filter: blur(24px) saturate(120%);
}

body.custom-background-active .reader-drawer > :is(
  .reader-catalog,
  .read-settings,
  .reader-bookshelf,
  .reader-source,
  .rule-manager,
  .cache-manager
) {
  background: transparent !important;
}

body.custom-background-active .source-modal :is(
  .source-manager-header,
  .source-list-wrapper,
  .editor-panel,
  .modal-footer
) {
  background: transparent !important;
}

body.custom-background-active .source-modal :is(
  .editor-tabs,
  .overview-card,
  .login-card,
  .source-url-block
),
body.custom-background-active .subscription-panel :is(
  .subscription-item
),
body.custom-background-active .webdav-modal :is(
  .backup-location,
  .path-bar code,
  .action-btn:not(.primary),
  .mini-btn
),
body.custom-background-active .cache-modal .cache-toolbar,
body.custom-background-active .detail-modal :is(.tag, .action-btn:not(.primary)) {
  background: var(--glass-inner-bg) !important;
  border-color: var(--glass-border-color) !important;
}

.app-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.app-main {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.app-main.without-header {
  height: auto;
  flex: 1;
  min-height: 0;
}

/* 直播全屏: 隐藏自绘标题栏与导航, 让直播间铺满整个窗口(窗口本身由 Tauri 设为全屏)。 */
body.live-fullscreen .titlebar,
body.live-fullscreen .app-topbar {
  display: none !important;
}

/* Teleported dialogs live outside .app-shell. Keep their fixed viewport below
   the custom titlebar so the draggable area never covers dialog content. */
:is(
  .modal-overlay,
  .modal-container,
  .drawer-overlay,
  .reader-overlay,
  .confirm-overlay,
  .login-preview-container,
  .subscription-backdrop
) {
  top: var(--titlebar-height, 30px) !important;
}

/* 全局固定底部播放条 */
.global-player-bar {
  position: fixed;
  bottom: 8px;
  left: 12px;
  right: 12px;
  z-index: 99;
  margin-top: 0 !important;
  max-width: 1200px;
  margin-left: auto !important;
  margin-right: auto !important;
}

/* Toast 高于播放条 */
.toast-container {
  z-index: 101;
}

/* 阅读页/直播全屏时播放条不占位（由 v-if 控制，无需额外 padding） */
</style>
