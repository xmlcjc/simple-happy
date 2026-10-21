<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="import-overlay" @click="close"></div>
    </Transition>
    <Transition name="fade">
      <div v-if="modelValue" class="import-card" role="dialog" aria-label="导入歌单">
        <header class="import-head">
          <strong>链接导入</strong>
          <button type="button" class="import-close" title="关闭" @click="close">×</button>
        </header>

        <div class="import-body">
          <label class="import-field">
            <span>音源</span>
            <select v-model="platform">
              <option
                v-for="plugin in store.enabledPlugins"
                :key="plugin.platform"
                :value="plugin.platform"
              >{{ plugin.name || plugin.platform }}</option>
            </select>
          </label>

          <label class="import-field">
            <span>类型</span>
            <select v-model="kind">
              <option value="sheet">歌单</option>
              <option value="item">单曲</option>
            </select>
          </label>

          <label class="import-field wide">
            <span>分享链接</span>
            <input
              v-model.trim="url"
              type="text"
              placeholder="粘贴歌单或单曲的分享链接 / 口令"
              maxlength="2048"
            />
          </label>

          <p v-if="errorText" class="import-error">{{ errorText }}</p>
        </div>

        <footer class="import-foot">
          <button type="button" class="import-btn ghost" @click="close">取消</button>
          <button
            type="button"
            class="import-btn primary"
            :disabled="importing || !url || !platform"
            @click="submit"
          >{{ importing ? '解析中…' : '导入' }}</button>
        </footer>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { importMusicItem, importMusicSheet, type MusicItem } from '../../api/music'
import { useMusicStore } from '../../stores/music'
import { useAppStore } from '../../stores/app'

const props = defineProps<{
  modelValue: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  imported: [items: MusicItem[], label: string]
}>()

const store = useMusicStore()
const appStore = useAppStore()

const platform = ref('')
const kind = ref<'sheet' | 'item'>('sheet')
const url = ref('')
const importing = ref(false)
const errorText = ref('')

watch(
  () => props.modelValue,
  (visible) => {
    if (!visible) return
    url.value = ''
    errorText.value = ''
    kind.value = 'sheet'
    platform.value = store.activePlatform || store.enabledPlugins[0]?.platform || ''
    void store.loadPlugins()
  },
)

function close() {
  emit('update:modelValue', false)
}

async function submit() {
  if (!url.value || !platform.value) return
  importing.value = true
  errorText.value = ''
  try {
    if (kind.value === 'sheet') {
      const items = await importMusicSheet(platform.value, url.value)
      if (!items.length) {
        errorText.value = '链接没有解析出歌曲，可能链接无效或插件不支持'
        return
      }
      appStore.showToast(`解析出 ${items.length} 首歌曲`, 'success')
      emit('imported', items, '导入歌单')
      close()
    } else {
      const item = await importMusicItem(platform.value, url.value)
      appStore.showToast('已解析出单曲', 'success')
      emit('imported', [item], '导入单曲')
      close()
    }
  } catch (error) {
    errorText.value = (error as Error).message || '导入失败'
  } finally {
    importing.value = false
  }
}
</script>

<style scoped>
.import-overlay {
  position: fixed;
  top: var(--titlebar-height, 30px);
  right: 0;
  bottom: 0;
  left: 0;
  z-index: var(--z-modal, 1000);
  background: rgba(0, 0, 0, 0.42);
  backdrop-filter: blur(3px);
}

.import-card {
  position: fixed;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  z-index: calc(var(--z-modal, 1000) + 1);
  width: min(420px, 92vw);
  display: flex;
  flex-direction: column;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
  box-shadow: 0 18px 50px rgba(0, 0, 0, 0.22);
  overflow: hidden;
}

.import-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-divider);
}

.import-head strong {
  color: var(--color-text);
  font-size: 14px;
}

.import-close {
  width: 26px;
  height: 26px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 18px;
  line-height: 1;
}

.import-close:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.import-body {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  padding: var(--space-4);
}

.import-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  flex: 1;
  min-width: 140px;
}

.import-field.wide {
  flex-basis: 100%;
}

.import-field span {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.import-field select,
.import-field input {
  width: 100%;
  padding: 7px 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  color: var(--color-text);
  font-size: 13px;
  outline: none;
}

.import-field select:focus,
.import-field input:focus {
  border-color: var(--color-primary);
}

.import-error {
  flex-basis: 100%;
  margin: 0;
  color: var(--color-danger, #c0392b);
  font-size: 12px;
  line-height: 1.5;
}

.import-foot {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-top: 1px solid var(--color-divider);
}

.import-btn {
  padding: 7px 16px;
  border-radius: var(--radius-md);
  font-size: 12px;
  font-weight: 600;
}

.import-btn.ghost {
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
}

.import-btn.ghost:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.import-btn.primary {
  background: var(--color-primary-soft-bg);
  color: var(--color-primary-soft-text);
}

.import-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
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
