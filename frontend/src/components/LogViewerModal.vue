<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="log-overlay" @click.self="close()">
        <div class="log-modal" role="dialog" aria-label="运行日志">
          <header class="log-header">
            <div>
              <h2>运行日志</h2>
              <p class="log-path" :title="path">{{ path || '加载中...' }}</p>
            </div>
            <button class="icon-btn" title="关闭" aria-label="关闭" @click="close()">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6 6 18M6 6l12 12" />
              </svg>
            </button>
          </header>

          <div class="log-toolbar">
            <div class="level-tabs" role="group" aria-label="日志级别">
              <button
                v-for="option in levelOptions"
                :key="option.value"
                type="button"
                :class="{ active: level === option.value }"
                @click="level = option.value"
              >{{ option.label }}</button>
            </div>
            <input
              v-model.trim="keyword"
              class="log-search"
              type="search"
              placeholder="按关键词 / 标签筛选"
              aria-label="筛选日志"
            />
            <span class="log-count">{{ visible.length }} / {{ entries.length }} 条</span>
          </div>

          <div class="log-body">
            <div v-if="loading" class="log-empty">正在读取日志...</div>
            <div v-else-if="filtered.length === 0" class="log-empty">
              {{ entries.length ? '没有匹配的日志' : '暂无日志，运行一段时间后再来看' }}
            </div>
            <div v-else class="log-list">
              <div
                v-for="(entry, index) in visible"
                :key="`${index}-${entry.time}`"
                class="log-row"
                :class="`level-${entry.level}`"
              >
                <span class="log-level">{{ entry.levelToken }}</span>
                <span class="log-time">{{ entry.time }}</span>
                <span class="log-tag" v-if="entry.tag">{{ entry.tag }}</span>
                <span class="log-message">{{ entry.message }}</span>
              </div>
              <div v-if="filtered.length > visibleLimit" class="log-more">
                仅显示最近 {{ visibleLimit }} 条，完整内容请下载
              </div>
            </div>
          </div>

          <footer class="log-footer">
            <button class="action-btn" type="button" @click="load">刷新</button>
            <button class="action-btn" type="button" :disabled="!visible.length" @click="copyVisible">复制</button>
            <button class="action-btn" type="button" :disabled="!filtered.length" @click="download">下载 TXT</button>
            <button class="action-btn danger" type="button" :disabled="clearing || !entries.length" @click="clearAll">
              {{ clearing ? '清理中...' : '清空日志' }}
            </button>
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useAppStore } from '../stores/app'
import { clearLogs, readLogs } from '../api/log'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()

interface LogEntry {
  level: 'debug' | 'info' | 'warn' | 'error'
  levelToken: string
  time: string
  tag: string
  message: string
}

const levelOptions = [
  { value: 'all', label: '全部' },
  { value: 'debug', label: '调试' },
  { value: 'info', label: '信息' },
  { value: 'warn', label: '警告' },
  { value: 'error', label: '错误' },
] as const

const appStore = useAppStore()
const path = ref('')
const entries = ref<LogEntry[]>([])
const level = ref<'all' | LogEntry['level']>('all')
const keyword = ref('')
const loading = ref(false)
const clearing = ref(false)
const visibleLimit = 500

const LEVELS: Record<string, LogEntry['level']> = {
  D: 'debug',
  I: 'info',
  W: 'warn',
  E: 'error',
}

/**
 * 解析两种格式：
 * - 前端写入：`[I] 2026-10-23 10:11:12.123 tag: message`
 * - tracing 默认：`2026-10-23T10:11:12.123456Z  INFO module::path: message`
 * 无法解析的行按 info 原文展示，保证任何日志都能被看到。
 */
function parseLine(line: string): LogEntry {
  const written = /^\[([DIWE])\]\s+(\S+\s+\S+)\s+([^:\s]+):\s?(.*)$/.exec(line)
  if (written) {
    const [, token, time, tag, message] = written
    return { level: LEVELS[token] ?? 'info', levelToken: token, time, tag, message }
  }

  const tracing = /^(\S+?)(?:\.\d+)?Z\s+([A-Z]+)\s+([^:]+):\s?(.*)$/.exec(line)
  if (tracing) {
    const [, timestamp, levelName, tag, message] = tracing
    const token = ({ DEBUG: 'D', INFO: 'I', WARN: 'W', ERROR: 'E' } as Record<string, string>)[levelName] ?? 'I'
    return {
      level: LEVELS[token] ?? 'info',
      levelToken: token,
      time: timestamp.replace('T', ' '),
      tag,
      message,
    }
  }

  return { level: 'info', levelToken: 'I', time: '', tag: '', message: line }
}

const filtered = computed(() => {
  const word = keyword.value.toLowerCase()
  return entries.value.filter((entry) => {
    if (level.value !== 'all' && entry.level !== level.value) return false
    if (!word) return true
    return `${entry.tag} ${entry.message}`.toLowerCase().includes(word)
  })
})

const visible = computed(() => filtered.value.slice(-visibleLimit))

async function load() {
  loading.value = true
  try {
    const snapshot = await readLogs(2000)
    path.value = snapshot.path || ''
    entries.value = (snapshot.lines || []).map(parseLine)
  } catch (error) {
    appStore.showToast((error as Error).message || '读取日志失败', 'error')
    entries.value = []
  } finally {
    loading.value = false
  }
}

function close() {
  emit('update:modelValue', false)
}

async function copyVisible() {
  const text = visible.value
    .map((entry) => `[${entry.levelToken}] ${entry.time} ${entry.tag}: ${entry.message}`)
    .join('\n')
  try {
    await navigator.clipboard.writeText(text)
    appStore.showToast(`已复制 ${visible.value.length} 条日志`, 'success')
  } catch {
    appStore.showToast('复制失败，请改用下载', 'error')
  }
}

function download() {
  const text = filtered.value
    .map((entry) => `[${entry.levelToken}] ${entry.time} ${entry.tag}: ${entry.message}`)
    .join('\n')
  const blob = new Blob([text], { type: 'text/plain;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)
  anchor.href = url
  anchor.download = `simple-happy-log-${stamp}.txt`
  anchor.click()
  URL.revokeObjectURL(url)
  appStore.showToast(`已导出 ${filtered.value.length} 条日志`, 'success')
}

async function clearAll() {
  const ok = await appStore.confirmDialog('确定清空应用日志吗？清空后无法恢复。', {
    title: '清空日志',
    danger: true,
  })
  if (!ok) return
  clearing.value = true
  try {
    await clearLogs()
    entries.value = []
    appStore.showToast('日志已清空', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '清空日志失败', 'error')
  } finally {
    clearing.value = false
  }
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) void load()
  },
)
</script>

<style scoped>
.log-overlay {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal, 200);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-6, 24px);
  background: rgb(15 23 42 / 0.45);
  backdrop-filter: blur(6px);
}

.log-modal {
  display: flex;
  flex-direction: column;
  width: min(980px, 100%);
  max-height: min(88vh, 900px);
  overflow: hidden;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-xl, 16px);
  box-shadow: var(--shadow-xl);
}

.log-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-4, 16px);
  padding: var(--space-5, 20px) var(--space-6, 24px) var(--space-3, 12px);
  border-bottom: 1px solid var(--color-divider);
}

.log-header h2 {
  margin: 0 0 4px;
  font-size: 17px;
  font-weight: 600;
}

.log-path {
  margin: 0;
  max-width: 640px;
  overflow: hidden;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: var(--radius-lg, 10px);
  color: var(--color-text-secondary);
}

.icon-btn:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.icon-btn svg {
  width: 18px;
  height: 18px;
}

.log-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3, 12px);
  padding: var(--space-3, 12px) var(--space-6, 24px);
  border-bottom: 1px solid var(--color-divider);
}

.level-tabs {
  display: flex;
  gap: 2px;
  padding: 3px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-full, 999px);
}

.level-tabs button {
  padding: 4px 10px;
  color: var(--color-text-secondary);
  font-size: 12px;
  border-radius: var(--radius-full, 999px);
}

.level-tabs button.active {
  color: var(--color-primary);
  background: var(--color-primary-bg);
  font-weight: 600;
}

.log-search {
  flex: 1;
  min-width: 200px;
  padding: 7px 12px;
  color: var(--color-text);
  font-size: 13px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg, 10px);
}

.log-count {
  color: var(--color-text-tertiary);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.log-body {
  flex: 1;
  min-height: 240px;
  overflow: auto;
  padding: var(--space-2, 8px) 0;
  background: var(--color-bg);
}

.log-empty {
  padding: 48px 24px;
  color: var(--color-text-tertiary);
  font-size: 13px;
  text-align: center;
}

.log-list {
  display: flex;
  flex-direction: column;
}

.log-row {
  display: flex;
  gap: 10px;
  padding: 4px var(--space-6, 24px);
  color: var(--color-text-secondary);
  font-family: var(--font-mono, ui-monospace, 'Cascadia Code', monospace);
  font-size: 12px;
  line-height: 1.6;
  word-break: break-all;
}

.log-row:hover {
  background: var(--color-bg-hover);
}

.log-level {
  flex: none;
  width: 14px;
  font-weight: 700;
}

.level-debug .log-level { color: var(--color-text-tertiary); }
.level-info .log-level { color: var(--color-primary); }
.level-warn .log-level { color: #d97706; }
.level-error .log-level { color: var(--color-danger, #dc2626); }
.level-error .log-message { color: var(--color-danger, #dc2626); }

.log-time {
  flex: none;
  width: 152px;
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}

.log-tag {
  flex: none;
  max-width: 140px;
  overflow: hidden;
  color: var(--color-text-tertiary);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log-message {
  flex: 1;
  min-width: 0;
}

.log-more {
  padding: 12px var(--space-6, 24px);
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.log-footer {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2, 8px);
  padding: var(--space-3, 12px) var(--space-6, 24px) var(--space-4, 16px);
  border-top: 1px solid var(--color-divider);
}

.action-btn {
  padding: 7px 14px;
  color: var(--color-text);
  font-size: 13px;
  background: var(--color-bg);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg, 10px);
}

.action-btn:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.action-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.action-btn.danger {
  color: var(--color-danger, #dc2626);
  border-color: color-mix(in srgb, var(--color-danger, #dc2626) 35%, transparent);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--duration-fast, 0.15s) var(--ease-out, ease);
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
