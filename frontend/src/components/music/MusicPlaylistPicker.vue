<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="modelValue" class="picker-overlay" @click="close"></div>
    </Transition>
    <Transition name="fade">
      <div v-if="modelValue" class="picker-card" role="dialog" aria-label="加入歌单">
        <header class="picker-head">
          <strong>加入歌单</strong>
          <small>{{ items.length }} 首歌曲</small>
          <button type="button" class="picker-close" title="关闭" @click="close">×</button>
        </header>

        <div class="picker-body">
          <p v-if="!libraryStore.playlists.length" class="picker-empty">还没有歌单，先创建一个。</p>
          <button
            v-for="playlist in libraryStore.playlists"
            :key="playlist.id"
            type="button"
            class="picker-row"
            :disabled="saving"
            @click="addTo(playlist.id)"
          >
            <span class="picker-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M4 6h10M4 12h10M4 18h6M18 15v5M20.5 17.5h-5" />
              </svg>
            </span>
            <span class="picker-text">
              <strong>{{ playlist.name }}</strong>
              <small>{{ playlist.itemCount }} 首</small>
            </span>
          </button>
        </div>

        <form class="picker-create" @submit.prevent="createAndAdd">
          <input v-model.trim="newName" type="text" placeholder="新建歌单名称" maxlength="40" />
          <button type="submit" :disabled="!newName || saving">
            {{ saving ? '处理中…' : '新建并加入' }}
          </button>
        </form>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useMusicLibraryStore } from '../../stores/musicLibrary'
import { useAppStore } from '../../stores/app'
import type { MusicItem } from '../../api/music'

const props = defineProps<{
  modelValue: boolean
  items: MusicItem[]
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  added: [count: number]
}>()

const libraryStore = useMusicLibraryStore()
const appStore = useAppStore()
const newName = ref('')
const saving = ref(false)

watch(
  () => props.modelValue,
  async (visible) => {
    if (!visible) return
    newName.value = ''
    try {
      await libraryStore.loadPlaylists()
    } catch (error) {
      appStore.showToast((error as Error).message || '加载歌单失败', 'error')
    }
  },
)

function close() {
  emit('update:modelValue', false)
}

async function addTo(playlistId: string) {
  saving.value = true
  try {
    const added = await libraryStore.addToPlaylist(playlistId, props.items)
    appStore.showToast(added > 0 ? `已加入 ${added} 首` : '歌曲已在歌单中', 'success')
    emit('added', added)
    close()
  } catch (error) {
    appStore.showToast((error as Error).message || '加入歌单失败', 'error')
  } finally {
    saving.value = false
  }
}

async function createAndAdd() {
  if (!newName.value) return
  saving.value = true
  try {
    const playlist = await libraryStore.createPlaylist(newName.value)
    await addTo(playlist.id)
  } catch (error) {
    appStore.showToast((error as Error).message || '新建歌单失败', 'error')
    saving.value = false
  }
}
</script>

<style scoped>
.picker-overlay {
  position: fixed;
  top: var(--titlebar-height, 30px);
  right: 0;
  bottom: 0;
  left: 0;
  z-index: var(--z-modal, 1000);
  background: rgba(0, 0, 0, 0.42);
  backdrop-filter: blur(3px);
}

.picker-card {
  position: fixed;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  z-index: calc(var(--z-modal, 1000) + 1);
  width: min(380px, 92vw);
  max-height: 70vh;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
  box-shadow: 0 18px 50px rgba(0, 0, 0, 0.22);
  overflow: hidden;
}

.picker-head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-divider);
}

.picker-head strong {
  color: var(--color-text);
  font-size: 14px;
}

.picker-head small {
  flex: 1;
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.picker-close {
  width: 26px;
  height: 26px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  font-size: 18px;
  line-height: 1;
}

.picker-close:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.picker-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--space-2);
}

.picker-empty {
  margin: var(--space-3);
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-align: center;
}

.picker-row {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  width: 100%;
  padding: 8px 10px;
  border-radius: var(--radius-md);
  text-align: left;
}

.picker-row:hover {
  background: var(--color-bg-hover);
}

.picker-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
  background: var(--color-bg-sunken);
  color: var(--color-text-secondary);
  flex-shrink: 0;
}

.picker-icon svg {
  width: 15px;
  height: 15px;
}

.picker-text {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.picker-text strong {
  color: var(--color-text);
  font-size: 13px;
  font-weight: 600;
}

.picker-text small {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.picker-create {
  display: flex;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-top: 1px solid var(--color-divider);
}

.picker-create input {
  flex: 1;
  min-width: 0;
  padding: 7px 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  color: var(--color-text);
  font-size: 13px;
  outline: none;
}

.picker-create input:focus {
  border-color: var(--color-primary);
}

.picker-create button {
  flex-shrink: 0;
  padding: 7px 12px;
  border-radius: var(--radius-md);
  background: var(--color-primary-soft-bg);
  color: var(--color-primary-soft-text);
  font-size: 12px;
  font-weight: 600;
}

.picker-create button:disabled {
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
