<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="vars-overlay" @click="close"></div>
    </Transition>
    <Transition name="fade">
      <div v-if="modelValue" class="vars-card" role="dialog" aria-label="插件设置">
        <header class="vars-head">
          <strong>{{ plugin?.name || plugin?.platform }} · 变量</strong>
          <button type="button" class="vars-close" title="关闭" @click="close">×</button>
        </header>

        <div class="vars-body">
          <p v-if="loading" class="vars-state">正在读取插件变量…</p>
          <p v-else-if="errorText" class="vars-state error">{{ errorText }}</p>
          <p v-else-if="!definitions.length" class="vars-state">该插件没有需要填写的变量。</p>
          <label v-for="variable in definitions" :key="variable.key" class="vars-field">
            <span>
              {{ variable.name || variable.key }}
              <small v-if="variable.hint">{{ variable.hint }}</small>
            </span>
            <input
              v-model="form[variable.key]"
              type="text"
              :placeholder="variable.name || variable.key"
              autocomplete="off"
            />
          </label>
        </div>

        <footer class="vars-foot">
          <button type="button" class="vars-btn ghost" @click="close">取消</button>
          <button
            type="button"
            class="vars-btn primary"
            :disabled="saving || loading || !definitions.length"
            @click="save"
          >{{ saving ? '保存中…' : '保存' }}</button>
        </footer>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  getMusicPluginVariableValues,
  getMusicPluginVariables,
  setMusicPluginVariables,
  type MusicPlugin,
  type MusicPluginVariable,
} from '../../api/music'
import { useAppStore } from '../../stores/app'

const props = defineProps<{
  modelValue: boolean
  plugin: MusicPlugin | null
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
}>()

const appStore = useAppStore()
const definitions = ref<MusicPluginVariable[]>([])
const form = ref<Record<string, string>>({})
const loading = ref(false)
const saving = ref(false)
const errorText = ref('')

watch(
  () => props.modelValue,
  async (visible) => {
    if (!visible || !props.plugin) return
    definitions.value = []
    form.value = {}
    errorText.value = ''
    loading.value = true
    try {
      const platform = props.plugin.platform
      const [defs, values] = await Promise.all([
        getMusicPluginVariables(platform),
        getMusicPluginVariableValues(platform),
      ])
      definitions.value = defs
      form.value = Object.fromEntries(
        defs.map((item) => [item.key, values[item.key] ?? '']),
      )
    } catch (error) {
      errorText.value = (error as Error).message || '读取插件变量失败'
    } finally {
      loading.value = false
    }
  },
)

function close() {
  emit('update:modelValue', false)
}

async function save() {
  if (!props.plugin) return
  saving.value = true
  try {
    await setMusicPluginVariables(props.plugin.platform, form.value)
    appStore.showToast('插件变量已保存，下次调用时生效', 'success')
    close()
  } catch (error) {
    appStore.showToast((error as Error).message || '保存失败', 'error')
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.vars-overlay {
  position: fixed;
  top: var(--titlebar-height, 30px);
  right: 0;
  bottom: 0;
  left: 0;
  z-index: var(--z-modal, 1000);
  background: rgba(0, 0, 0, 0.42);
  backdrop-filter: blur(3px);
}

.vars-card {
  position: fixed;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  z-index: calc(var(--z-modal, 1000) + 1);
  width: min(420px, 92vw);
  max-height: 76vh;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
  box-shadow: 0 18px 50px rgba(0, 0, 0, 0.22);
  overflow: hidden;
}

.vars-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-divider);
}

.vars-head strong {
  color: var(--color-text);
  font-size: 14px;
}

.vars-close {
  width: 26px;
  height: 26px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 18px;
  line-height: 1;
}

.vars-close:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.vars-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-4);
}

.vars-state {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-align: center;
}

.vars-state.error {
  color: var(--color-danger, #c0392b);
  text-align: left;
}

.vars-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.vars-field span {
  display: flex;
  flex-direction: column;
  gap: 2px;
  color: var(--color-text-secondary);
  font-size: 12px;
}

.vars-field small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.vars-field input {
  width: 100%;
  padding: 7px 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  color: var(--color-text);
  font-size: 13px;
  outline: none;
}

.vars-field input:focus {
  border-color: var(--color-primary);
}

.vars-foot {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-top: 1px solid var(--color-divider);
}

.vars-btn {
  padding: 7px 16px;
  border-radius: var(--radius-md);
  font-size: 12px;
  font-weight: 600;
}

.vars-btn.ghost {
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
}

.vars-btn.ghost:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.vars-btn.primary {
  background: var(--color-primary-soft-bg);
  color: var(--color-primary-soft-text);
}

.vars-btn:disabled {
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
