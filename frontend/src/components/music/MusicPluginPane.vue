<template>
  <div class="music-pane">
    <div class="plugin-head">
      <div class="plugin-head-copy">
        <strong>MusicFree 插件</strong>
        <small>软件本身不内置音源；安装插件后即可搜索、播放与获取歌词。</small>
      </div>
      <button type="button" class="install-btn" :disabled="installing" @click="fileInput?.click()">
        {{ installing ? '正在安装…' : '安装插件' }}
      </button>
      <input
        ref="fileInput"
        class="hidden-input"
        type="file"
        accept=".js,text/javascript,application/javascript"
        @change="handleFile"
      />
    </div>

    <p v-if="errorText" class="plugin-error">{{ errorText }}</p>

    <div v-if="store.pluginsLoading" class="music-state"><p>正在加载插件…</p></div>
    <div v-else-if="!store.plugins.length" class="music-state">
      <p>还没有安装插件</p>
      <small>选择一个 MusicFree 的 .js 插件文件即可安装。</small>
    </div>

    <div v-else class="plugin-list">
      <div v-for="plugin in store.plugins" :key="plugin.platform" class="plugin-row">
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
          class="switch-control"
          :class="{ on: plugin.enabled }"
          type="button"
          role="switch"
          :aria-checked="plugin.enabled"
          :aria-label="`启用 ${plugin.name || plugin.platform}`"
          @click="togglePlugin(plugin)"
        >
          <span></span>
        </button>
        <button type="button" class="remove-btn" @click="removePlugin(plugin)">卸载</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useMusicStore } from '../../stores/music'
import { useAppStore } from '../../stores/app'
import type { MusicPlugin } from '../../api/music'

const store = useMusicStore()
const appStore = useAppStore()

const fileInput = ref<HTMLInputElement | null>(null)
const installing = ref(false)
const errorText = ref('')

async function handleFile(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  installing.value = true
  errorText.value = ''
  try {
    const source = await file.text()
    const plugin = await store.installPlugin(source, file.name)
    appStore.showToast(`插件「${plugin.name || plugin.platform}」已安装`, 'success')
  } catch (error) {
    errorText.value = (error as Error).message || '插件安装失败'
  } finally {
    installing.value = false
    input.value = ''
  }
}

async function togglePlugin(plugin: MusicPlugin) {
  try {
    await store.togglePlugin(plugin.platform, !plugin.enabled)
  } catch (error) {
    appStore.showToast((error as Error).message || '切换插件状态失败', 'error')
  }
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
  align-items: center;
  gap: var(--space-3);
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

.remove-btn {
  flex-shrink: 0;
  padding: 4px 10px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.remove-btn:hover {
  background: var(--color-bg-hover);
  color: var(--color-danger, #c0392b);
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
</style>
