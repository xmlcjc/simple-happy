<template>
  <div class="music-pane">
    <div class="plugin-head">
      <div class="plugin-head-copy">
        <strong>MusicFree 插件</strong>
        <small>软件本身不内置音源；安装插件后即可搜索、播放与获取歌词。</small>
      </div>
      <div class="plugin-head-actions">
        <button type="button" class="install-btn ghost" @click="openSubscriptions">
          订阅
        </button>
        <button
          v-if="hasUpdatable"
          type="button"
          class="install-btn ghost"
          :disabled="updatingAll"
          @click="updateAll"
        >{{ updatingAll ? '更新中…' : '全部更新' }}</button>
        <button type="button" class="install-btn ghost" @click="openUrlDialog">
          网络安装
        </button>
        <button
          type="button"
          class="install-btn"
          :disabled="installing"
          @click="fileInput?.click()"
        >{{ installing ? '正在安装…' : '安装插件' }}</button>
      </div>
      <input
        ref="fileInput"
        class="hidden-input"
        type="file"
        accept=".js,text/javascript,application/javascript"
        multiple
        @change="handleFiles"
      />
    </div>

    <p v-if="errorText" class="plugin-error">{{ errorText }}</p>

    <div v-if="store.pluginsLoading" class="music-state"><p>正在加载插件…</p></div>
    <div v-else-if="!store.plugins.length" class="music-state">
      <p>还没有安装插件</p>
      <small>支持本地 .js 文件（可多选）、网络地址与 .json 订阅清单。</small>
      <div class="empty-actions">
        <button type="button" class="install-btn" @click="fileInput?.click()">选择本地文件</button>
        <button type="button" class="install-btn ghost" @click="openUrlDialog">从网络安装</button>
        <button type="button" class="install-btn ghost" @click="openSubscriptions">管理订阅</button>
      </div>
    </div>
    <div v-else class="plugin-list">
      <div v-for="(plugin, index) in store.plugins" :key="plugin.platform" class="plugin-row">
        <span class="plugin-avatar">{{ (plugin.name || plugin.platform).slice(0, 1) }}</span>
        <span class="plugin-text">
          <strong>{{ plugin.name || plugin.platform }}</strong>
          <small>
            {{ plugin.platform }}
            <template v-if="plugin.version"> · v{{ plugin.version }}</template>
            <template v-if="plugin.author"> · {{ plugin.author }}</template>
          </small>
        </span>
        <button
          v-if="plugin.description"
          type="button"
          class="icon-btn"
          title="插件说明"
          @click="openDescription(plugin)"
        >?</button>
        <span class="order-buttons">
          <button
            type="button"
            class="icon-btn"
            title="上移"
            :disabled="index === 0"
            @click="move(plugin, -1)"
          >↑</button>
          <button
            type="button"
            class="icon-btn"
            title="下移"
            :disabled="index === store.plugins.length - 1"
            @click="move(plugin, 1)"
          >↓</button>
        </span>
        <button
          v-if="plugin.srcUrl"
          type="button"
          class="vars-btn"
          :disabled="updating.includes(plugin.platform)"
          title="从插件源地址更新"
          @click="updateOne(plugin)"
        >{{ updating.includes(plugin.platform) ? '更新中' : '更新' }}</button>
        <button type="button" class="vars-btn" title="填写插件变量" @click="openVariables(plugin)">
          变量
        </button>
        <select
          v-if="alternativeOptions(plugin).length"
          class="alt-select"
          :value="settings.alternativeMap[plugin.platform] || ''"
          :aria-label="`${plugin.name || plugin.platform} 的替代音源`"
          @change="changeAlternative(plugin.platform, $event)"
        >
          <option value="">不替代</option>
          <option
            v-for="option in alternativeOptions(plugin)"
            :key="option.platform"
            :value="option.platform"
          >{{ option.name || option.platform }}</option>
        </select>
        <button
          type="button"
          class="switch-control"
          :class="{ on: plugin.enabled }"
          role="switch"
          :aria-checked="plugin.enabled"
          :aria-label="`启用 ${plugin.name || plugin.platform}`"
          @click="togglePlugin(plugin)"
        ><span /></button>
        <button
          v-if="plugin.srcUrl"
          type="button"
          class="remove-btn"
          title="复制插件地址"
          @click="sharePlugin(plugin)"
        >分享</button>
        <button type="button" class="remove-btn" @click="removePlugin(plugin)">卸载</button>
      </div>
      <div class="plugin-foot">
        <small>共 {{ store.plugins.length }} 个插件</small>
        <button type="button" class="uninstall-all" @click="uninstallAll">卸载全部</button>
      </div>
    </div>

    <MusicPluginVariablesDialog v-model="varsOpen" :plugin="varsPlugin" />

    <!-- 从网络安装 -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="urlOpen" class="plugin-overlay" @click="urlOpen = false"></div>
      </Transition>
      <Transition name="fade">
        <div v-if="urlOpen" class="plugin-card" role="dialog" aria-label="从网络安装插件">
          <header class="plugin-card-head">
            <strong>从网络安装插件</strong>
            <button type="button" class="plugin-close" title="关闭" @click="urlOpen = false">×</button>
          </header>
          <div class="plugin-card-body">
            <input
              v-model="urlInput"
              class="plugin-input"
              type="url"
              placeholder="https://example.com/plugin.js 或 plugins.json"
              @keyup.enter="installFromUrl"
            />
            <small class="plugin-hint">
              支持单个 .js 直链；.json 订阅清单（{plugins:[{url}]}）会批量安装。
            </small>
          </div>
          <footer class="plugin-card-foot">
            <button type="button" class="plugin-btn ghost" @click="urlOpen = false">取消</button>
            <button
              type="button"
              class="plugin-btn primary"
              :disabled="urlInstalling || !urlInput.trim()"
              @click="installFromUrl"
            >{{ urlInstalling ? '安装中…' : '安装' }}</button>
          </footer>
        </div>
      </Transition>
    </Teleport>

    <!-- 安装 / 更新结果 -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="resultOpen" class="plugin-overlay top" @click="resultOpen = false"></div>
      </Transition>
      <Transition name="fade">
        <div v-if="resultOpen" class="plugin-card wide top" role="dialog" aria-label="安装结果">
          <header class="plugin-card-head">
            <strong>{{ resultTitle }}</strong>
            <button type="button" class="plugin-close" title="关闭" @click="resultOpen = false">×</button>
          </header>
          <div class="plugin-card-body">
            <p v-if="!resultItems.length" class="plugin-hint">没有需要处理的结果。</p>
            <div
              v-for="(item, index) in resultItems"
              :key="`${item.url}-${index}`"
              class="result-row"
              :class="{ failed: !item.success }"
            >
              <span class="result-state">{{ item.success ? '成功' : '失败' }}</span>
              <span class="result-text">
                <strong>{{ item.name || item.url }}</strong>
                <small v-if="item.success">
                  {{ item.platform }}<template v-if="item.version"> · v{{ item.version }}</template>
                </small>
                <small v-else>{{ item.error || '未知错误' }}</small>
              </span>
            </div>
          </div>
          <footer class="plugin-card-foot">
            <button type="button" class="plugin-btn primary" @click="resultOpen = false">知道了</button>
          </footer>
        </div>
      </Transition>
    </Teleport>

    <!-- 订阅管理 -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="subsOpen" class="plugin-overlay" @click="subsOpen = false"></div>
      </Transition>
      <Transition name="fade">
        <div v-if="subsOpen" class="plugin-card wide" role="dialog" aria-label="插件订阅">
          <header class="plugin-card-head">
            <strong>插件订阅</strong>
            <button type="button" class="plugin-close" title="关闭" @click="subsOpen = false">×</button>
          </header>
          <div class="plugin-card-body">
            <p v-if="store.subscriptionsLoading" class="plugin-hint">正在加载订阅…</p>
            <p v-else-if="!store.subscriptions.length" class="plugin-hint">
              还没有订阅源。添加 .js 直链或 .json 订阅清单后，可一键批量更新。
            </p>
            <div
              v-for="item in store.subscriptions"
              :key="item.id"
              class="subs-row"
            >
              <span class="subs-text">
                <strong>{{ item.name }}</strong>
                <small>{{ item.url }}</small>
              </span>
              <button
                type="button"
                class="vars-btn"
                :disabled="subUpdating === item.id"
                @click="updateSubscription(item.id)"
              >{{ subUpdating === item.id ? '更新中' : '更新' }}</button>
              <button type="button" class="remove-btn" @click="removeSubscription(item)">删除</button>
            </div>
            <div class="subs-add">
              <input v-model="subName" class="plugin-input" type="text" placeholder="名称（可选）" />
              <input
                v-model="subUrl"
                class="plugin-input"
                type="url"
                placeholder="https://example.com/plugins.json"
                @keyup.enter="addSubscription"
              />
              <button
                type="button"
                class="plugin-btn primary"
                :disabled="subAdding || !subUrl.trim()"
                @click="addSubscription"
              >{{ subAdding ? '添加中…' : '添加' }}</button>
            </div>
          </div>
          <footer class="plugin-card-foot">
            <button
              type="button"
              class="plugin-btn ghost"
              :disabled="subsUpdatingAll || !store.subscriptions.length"
              @click="updateAllSubscriptions"
            >{{ subsUpdatingAll ? '更新中…' : '更新全部订阅' }}</button>
            <button type="button" class="plugin-btn primary" @click="subsOpen = false">关闭</button>
          </footer>
        </div>
      </Transition>
    </Teleport>

    <!-- 插件说明 -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="descOpen" class="plugin-overlay" @click="descOpen = false"></div>
      </Transition>
      <Transition name="fade">
        <div v-if="descOpen" class="plugin-card" role="dialog" aria-label="插件说明">
          <header class="plugin-card-head">
            <strong>{{ descPlugin?.name || descPlugin?.platform }} · 说明</strong>
            <button type="button" class="plugin-close" title="关闭" @click="descOpen = false">×</button>
          </header>
          <div class="plugin-card-body">
            <pre class="plugin-description">{{ descPlugin?.description }}</pre>
          </div>
          <footer class="plugin-card-foot">
            <button type="button" class="plugin-btn primary" @click="descOpen = false">关闭</button>
          </footer>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useMusicStore } from '../../stores/music'
import { useMusicSettingsStore } from '../../stores/musicSettings'
import { useAppStore } from '../../stores/app'
import MusicPluginVariablesDialog from './MusicPluginVariablesDialog.vue'
import type { MusicPlugin, MusicPluginInstallResult } from '../../api/music'

const store = useMusicStore()
const settings = useMusicSettingsStore()
const appStore = useAppStore()

const fileInput = ref<HTMLInputElement | null>(null)
const installing = ref(false)
const errorText = ref('')
const varsOpen = ref(false)
const varsPlugin = ref<MusicPlugin | null>(null)
const updating = ref<string[]>([])
const updatingAll = ref(false)

const hasUpdatable = computed(() =>
  store.plugins.some((plugin) => Boolean(plugin.srcUrl?.trim())),
)

/* ── 网络安装 ── */
const urlOpen = ref(false)
const urlInput = ref('')
const urlInstalling = ref(false)

function openUrlDialog() {
  urlInput.value = ''
  urlOpen.value = true
}

async function installFromUrl() {
  const url = urlInput.value.trim()
  if (!url) return
  urlInstalling.value = true
  try {
    const results = await store.installPluginFromUrl(url)
    urlOpen.value = false
    showResults('安装结果', results)
  } catch (error) {
    appStore.showToast((error as Error).message || '安装失败', 'error')
  } finally {
    urlInstalling.value = false
  }
}

/* ── 结果汇总 ── */
const resultOpen = ref(false)
const resultTitle = ref('')
const resultItems = ref<MusicPluginInstallResult[]>([])

/** 全部成功只提示一句；有失败则弹窗逐条展示原因（与 MusicFree 一致）。 */
function showResults(title: string, results: MusicPluginInstallResult[]) {
  const failures = results.filter((item) => !item.success)
  if (!failures.length) {
    appStore.showToast(`已处理 ${results.length} 个插件`, 'success')
    return
  }
  resultTitle.value = title
  resultItems.value = results
  resultOpen.value = true
}

/* ── 本地安装（多选） ── */
async function handleFiles(event: Event) {
  const input = event.target as HTMLInputElement
  const files = Array.from(input.files ?? [])
  if (!files.length) return
  installing.value = true
  errorText.value = ''
  const results: MusicPluginInstallResult[] = []
  try {
    for (const file of files) {
      const url = file.name
      try {
        const source = await file.text()
        const plugin = await store.installPlugin(source, file.name)
        results.push({
          url,
          success: true,
          platform: plugin.platform,
          name: plugin.name,
          version: plugin.version,
          error: '',
        })
      } catch (error) {
        results.push({
          url,
          success: false,
          platform: '',
          name: file.name,
          version: '',
          error: (error as Error).message || '安装失败',
        })
      }
    }
    if (results.length === 1) {
      const only = results[0]
      if (only.success) {
        appStore.showToast(`插件「${only.name || only.platform}」已安装`, 'success')
      } else {
        errorText.value = only.error
      }
    } else {
      showResults('安装结果', results)
    }
  } finally {
    installing.value = false
    input.value = ''
  }
}

/* ── 单个 / 全部更新 ── */
async function updateOne(plugin: MusicPlugin) {
  const previous = plugin.version
  updating.value = [...updating.value, plugin.platform]
  try {
    const next = await store.updatePlugin(plugin.platform)
    appStore.showToast(
      `「${next.name || next.platform}」已更新 ${previous || '?'} → ${next.version || '?'}`,
      'success',
    )
  } catch (error) {
    appStore.showToast((error as Error).message || '插件更新失败', 'error')
  } finally {
    updating.value = updating.value.filter((platform) => platform !== plugin.platform)
  }
}

async function updateAll() {
  updatingAll.value = true
  try {
    const result = await store.updateAllPlugins()
    const failures: MusicPluginInstallResult[] = result.failures.map((item) => ({
      url: '',
      success: false,
      platform: item.platform,
      name: item.platform,
      version: '',
      error: item.error,
    }))
    if (!result.updated && !failures.length) {
      appStore.showToast('没有可更新的插件', 'warning')
      return
    }
    appStore.showToast(`更新成功 ${result.updated} 个`, 'success')
    if (failures.length) {
      // 失败的逐条列出来源与原因; 成功的只计入上面的统计。
      resultTitle.value = '更新结果'
      resultItems.value = failures
      resultOpen.value = true
    }
  } catch (error) {
    appStore.showToast((error as Error).message || '更新失败', 'error')
  } finally {
    updatingAll.value = false
  }
}

/* ── 订阅 ── */
const subsOpen = ref(false)
const subName = ref('')
const subUrl = ref('')
const subAdding = ref(false)
const subUpdating = ref<number | null>(null)
const subsUpdatingAll = ref(false)

async function openSubscriptions() {
  subsOpen.value = true
  subName.value = ''
  subUrl.value = ''
  try {
    await store.loadSubscriptions()
  } catch (error) {
    appStore.showToast((error as Error).message || '读取订阅失败', 'error')
  }
}

async function addSubscription() {
  const url = subUrl.value.trim()
  if (!url) return
  subAdding.value = true
  try {
    await store.addSubscription(subName.value.trim(), url)
    subName.value = ''
    subUrl.value = ''
    appStore.showToast('订阅已添加', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '添加订阅失败', 'error')
  } finally {
    subAdding.value = false
  }
}

async function removeSubscription(item: { id: number; name: string }) {
  const confirmed = await appStore.confirmDialog(
    `删除订阅「${item.name}」？\n已安装的插件不会被卸载。`,
    { title: '删除订阅', danger: true },
  )
  if (!confirmed) return
  try {
    await store.removeSubscription(item.id)
    appStore.showToast('订阅已删除', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '删除失败', 'error')
  }
}

async function updateSubscription(id: number) {
  subUpdating.value = id
  try {
    const results = await store.updateSubscription(id)
    showResults('订阅更新结果', results)
  } catch (error) {
    appStore.showToast((error as Error).message || '更新订阅失败', 'error')
  } finally {
    subUpdating.value = null
  }
}

async function updateAllSubscriptions() {
  subsUpdatingAll.value = true
  try {
    const results = await store.updateAllSubscriptions()
    showResults('订阅更新结果', results)
  } catch (error) {
    appStore.showToast((error as Error).message || '更新订阅失败', 'error')
  } finally {
    subsUpdatingAll.value = false
  }
}

/* ── 排序 / 说明 / 分享 ── */
async function move(plugin: MusicPlugin, offset: number) {
  try {
    await store.movePlugin(plugin.platform, offset)
  } catch (error) {
    appStore.showToast((error as Error).message || '排序失败', 'error')
  }
}

const descOpen = ref(false)
const descPlugin = ref<MusicPlugin | null>(null)

function openDescription(plugin: MusicPlugin) {
  descPlugin.value = plugin
  descOpen.value = true
}

async function sharePlugin(plugin: MusicPlugin) {
  try {
    await navigator.clipboard.writeText(plugin.srcUrl)
    appStore.showToast('插件地址已复制到剪贴板', 'success')
  } catch {
    appStore.showToast('复制失败，请手动复制插件地址', 'error')
  }
}

async function uninstallAll() {
  const confirmed = await appStore.confirmDialog(
    `卸载全部 ${store.plugins.length} 个插件？\n该操作会删除本地插件文件，订阅源会保留。`,
    { title: '卸载全部插件', danger: true },
  )
  if (!confirmed) return
  try {
    const result = await store.uninstallAllPlugins()
    appStore.showToast(`已卸载 ${result.removed} 个插件`, 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '卸载失败', 'error')
  }
}

/* ── 变量 / 替代源 / 启停 / 卸载 ── */
function openVariables(plugin: MusicPlugin) {
  varsPlugin.value = plugin
  varsOpen.value = true
}

async function togglePlugin(plugin: MusicPlugin) {
  try {
    await store.togglePlugin(plugin.platform, !plugin.enabled)
  } catch (error) {
    appStore.showToast((error as Error).message || '切换插件状态失败', 'error')
  }
}

/** 可换源的候选：其它已启用的插件。 */
function alternativeOptions(plugin: MusicPlugin) {
  return store.enabledPlugins.filter((item) => item.platform !== plugin.platform)
}

function changeAlternative(platform: string, event: Event) {
  const value = (event.target as HTMLSelectElement).value
  settings.setAlternative(platform, value)
  appStore.showToast(value ? '已设置替代音源' : '已关闭替代播放', 'success')
}

async function removePlugin(plugin: MusicPlugin) {
  const confirmed = await appStore.confirmDialog(
    `卸载插件「${plugin.name || plugin.platform}」？\n该操作会删除本地插件文件。`,
    { title: '卸载插件', danger: true },
  )
  if (!confirmed) return
  try {
    await store.uninstallPlugin(plugin.platform)
    appStore.showToast('插件已卸载', 'success')
  } catch (error) {
    appStore.showToast((error as Error).message || '卸载失败', 'error')
  }
}
</script>

<style scoped>
.music-pane {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.plugin-head {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
  flex-wrap: wrap;
}

.plugin-head-copy {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  flex: 1;
}

.plugin-head-copy strong {
  color: var(--color-text);
  font-size: 14px;
}

.plugin-head-copy small {
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.plugin-head-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}

.install-btn {
  flex-shrink: 0;
  padding: 7px 16px;
  border-radius: var(--radius-md);
  background: var(--color-primary-soft-bg);
  color: var(--color-primary-soft-text);
  font-size: var(--text-sm);
  font-weight: 600;
}

.install-btn:disabled {
  opacity: 0.55;
  cursor: wait;
}

.install-btn.ghost {
  border: 1px solid var(--color-border-light);
  background: transparent;
  color: var(--color-text-secondary);
}

.install-btn.ghost:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.hidden-input {
  display: none;
}

.plugin-error {
  margin: 0;
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-md);
  background: var(--color-danger-bg, rgba(220, 80, 80, 0.1));
  color: var(--color-danger, #c0392b);
  font-size: 12px;
  line-height: 1.6;
}

.plugin-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-y: auto;
  min-height: 0;
}

.plugin-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  padding: 10px 12px;
  border-radius: var(--radius-md);
  border: 1px solid transparent;
}

.plugin-row:hover {
  border-color: var(--color-border-light);
  background: var(--color-bg-elevated);
}

.plugin-avatar {
  width: 36px;
  height: 36px;
  border-radius: var(--radius-md);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  background: var(--color-bg-sunken);
  color: var(--color-text-secondary);
  font-size: 15px;
  font-weight: 600;
}

.plugin-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.plugin-text strong,
.plugin-text small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.plugin-text strong {
  color: var(--color-text);
  font-size: 14px;
  font-weight: 600;
}

.plugin-text small {
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.icon-btn {
  width: 24px;
  height: 24px;
  flex-shrink: 0;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-tertiary);
  font-size: 12px;
  line-height: 1;
}

.icon-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.icon-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.order-buttons {
  display: inline-flex;
  gap: 2px;
  flex-shrink: 0;
}

.remove-btn {
  flex-shrink: 0;
  padding: 4px 10px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.vars-btn {
  flex-shrink: 0;
  padding: 4px 10px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.vars-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.vars-btn:disabled {
  opacity: 0.55;
  cursor: wait;
}

.alt-select {
  flex-shrink: 0;
  max-width: 132px;
  padding: 4px 6px;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  color: var(--color-text-secondary);
  font-size: 11px;
  outline: none;
}

.alt-select:focus {
  border-color: var(--color-primary);
}

.remove-btn:hover {
  background: var(--color-bg-hover);
  color: var(--color-danger, #c0392b);
}

.plugin-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.uninstall-all {
  color: var(--color-text-tertiary);
  font-size: 12px;
  padding: 4px 10px;
  border-radius: var(--radius-md);
}

.uninstall-all:hover {
  background: var(--color-bg-hover);
  color: var(--color-danger, #c0392b);
}

.empty-actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  justify-content: center;
  margin-top: var(--space-2);
}

.music-state {
  padding: 64px 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  color: var(--color-text-tertiary);
  font-size: 13px;
  text-align: center;
}

.music-state p {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: 14px;
}

.switch-control {
  position: relative;
  width: 38px;
  height: 22px;
  border-radius: 999px;
  flex-shrink: 0;
  background: var(--color-bg-sunken);
  border: 1px solid var(--color-border-light);
  transition: background var(--duration-fast) var(--ease-out);
}

.switch-control span {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--color-bg-elevated);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: transform var(--duration-fast) var(--ease-out);
}

.switch-control.on {
  background: var(--color-primary);
  border-color: var(--color-primary);
}

.switch-control.on span {
  transform: translateX(16px);
}

/* ── 通用弹窗 ── */
.plugin-overlay {
  position: fixed;
  top: var(--titlebar-height, 30px);
  right: 0;
  bottom: 0;
  left: 0;
  z-index: var(--z-modal, 1000);
  background: rgba(0, 0, 0, 0.42);
  backdrop-filter: blur(3px);
}

.plugin-card {
  position: fixed;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  z-index: calc(var(--z-modal, 1000) + 1);
  width: min(460px, 92vw);
  max-height: 76vh;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
  box-shadow: 0 18px 50px rgba(0, 0, 0, 0.22);
  overflow: hidden;
}

.plugin-card.wide {
  width: min(600px, 94vw);
}

/* 结果弹窗会叠在订阅弹窗之上，关闭后还能继续管理订阅。 */
.plugin-overlay.top {
  z-index: calc(var(--z-modal, 1000) + 2);
}

.plugin-card.top {
  z-index: calc(var(--z-modal, 1000) + 3);
}

.plugin-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-divider);
}

.plugin-card-head strong {
  color: var(--color-text);
  font-size: 14px;
}

.plugin-close {
  width: 26px;
  height: 26px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 18px;
  line-height: 1;
}

.plugin-close:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.plugin-card-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-4);
}

.plugin-card-foot {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-top: 1px solid var(--color-divider);
}

.plugin-input {
  width: 100%;
  padding: 7px 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  color: var(--color-text);
  font-size: 13px;
  outline: none;
}

.plugin-input:focus {
  border-color: var(--color-primary);
}

.plugin-hint {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: 11px;
  line-height: 1.6;
}

.plugin-btn {
  padding: 7px 16px;
  border-radius: var(--radius-md);
  font-size: 12px;
  font-weight: 600;
}

.plugin-btn.ghost {
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
}

.plugin-btn.ghost:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.plugin-btn.primary {
  background: var(--color-primary-soft-bg);
  color: var(--color-primary-soft-text);
}

.plugin-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.result-row,
.subs-row {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: 8px 10px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
}

.result-row.failed {
  border-color: var(--color-danger-border, rgba(192, 57, 43, 0.35));
  background: var(--color-danger-bg, rgba(220, 80, 80, 0.08));
}

.result-state {
  flex-shrink: 0;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11px;
  background: var(--color-bg-sunken);
  color: var(--color-text-secondary);
}

.result-row.failed .result-state {
  color: var(--color-danger, #c0392b);
}

.result-text,
.subs-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
  word-break: break-all;
}

.result-text strong,
.subs-text strong {
  color: var(--color-text);
  font-size: 13px;
}

.result-text small,
.subs-text small {
  color: var(--color-text-tertiary);
  font-size: 11px;
  line-height: 1.5;
}

.subs-row {
  align-items: center;
}

.subs-add {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  flex-wrap: wrap;
  padding-top: var(--space-2);
  border-top: 1px dashed var(--color-divider);
}

.subs-add .plugin-input {
  flex: 1;
  min-width: 160px;
}

.plugin-description {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--color-text-secondary);
  font-family: inherit;
  font-size: 12px;
  line-height: 1.7;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--duration-fast) var(--ease-out);
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
