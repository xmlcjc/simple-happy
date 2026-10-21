<template>
  <section class="detail-panel" aria-label="详情">
    <header class="detail-head">
      <button type="button" class="back-btn" title="返回" @click="emit('close')">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="m15 18-6-6 6-6" />
        </svg>
      </button>
      <img
        v-if="display.artwork"
        class="detail-cover"
        :src="display.artwork"
        :alt="title"
        referrerpolicy="no-referrer"
      />
      <span v-else class="detail-cover placeholder">{{ title.slice(0, 1) }}</span>
      <div class="detail-copy">
        <h2>{{ title }}</h2>
        <p>{{ subtitle }}</p>
        <p v-if="display.description" class="detail-desc">{{ display.description }}</p>
      </div>
      <button
        v-if="listItems.length"
        type="button"
        class="play-all"
        @click="playAll"
      >播放全部</button>
    </header>

    <div v-if="isArtist" class="detail-tabs" role="tablist" aria-label="作品类型">
      <button
        type="button"
        role="tab"
        :aria-selected="workType === 'music'"
        :class="{ active: workType === 'music' }"
        @click="switchWorkType('music')"
      >单曲</button>
      <button
        type="button"
        role="tab"
        :aria-selected="workType === 'album'"
        :class="{ active: workType === 'album' }"
        @click="switchWorkType('album')"
      >专辑</button>
    </div>

    <div class="detail-body">
      <p v-if="loading && !detail" class="detail-state">正在加载…</p>
      <p v-else-if="error" class="detail-state">{{ error }}</p>

      <MusicCollectionList
        v-else-if="isArtist && workType === 'album'"
        :items="listCollections"
        empty-text="这位歌手暂无专辑"
        @open="emit('open-collection', $event, 'album')"
      ></MusicCollectionList>

      <MusicTrackList
        v-else
        :items="listItems"
        :empty-text="emptyText"
        :show-source="false"
        @play="emit('play', $event, listItems)"
        @play-next="emit('play-next', $event)"
      >
        <template #actions="{ item }">
          <button
            type="button"
            class="row-action"
            title="加入歌单"
            aria-label="加入歌单"
            @click.stop="emit('add-to-playlist', item)"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 5v14M5 12h14" />
            </svg>
          </button>
        </template>
      </MusicTrackList>

      <div v-if="hasMore" class="more-row">
        <button type="button" class="state-btn" :disabled="loading" @click="load(false)">
          {{ loading ? '加载中…' : '加载更多' }}
        </button>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import {
  getMusicCollectionDetail,
  type MusicCollectionDetail,
  type MusicCollectionItem,
  type MusicCollectionKind,
  type MusicItem,
} from '../../api/music'
import MusicTrackList from './MusicTrackList.vue'
import MusicCollectionList from './MusicCollectionList.vue'

const props = defineProps<{
  platform: string
  kind: MusicCollectionKind
  item: MusicCollectionItem
}>()

const emit = defineEmits<{
  close: []
  play: [item: MusicItem, queue: MusicItem[]]
  'play-next': [item: MusicItem]
  'add-to-playlist': [item: MusicItem]
  'open-collection': [item: MusicCollectionItem, kind: MusicCollectionKind]
}>()

const detail = ref<MusicCollectionDetail | null>(null)
const loading = ref(false)
const error = ref('')
const page = ref(1)
const hasMore = ref(false)
const workType = ref<'music' | 'album'>('music')

const display = computed<MusicCollectionItem>(() => detail.value?.item ?? props.item)
const title = computed(() => display.value.title || '详情')
const isArtist = computed(() => props.kind === 'artist')
const listItems = computed(() => detail.value?.items ?? [])
const listCollections = computed(() => detail.value?.collections ?? [])

const subtitle = computed(() => {
  const parts: string[] = []
  if (props.kind === 'album') {
    if (display.value.artist) parts.push(display.value.artist)
    if (display.value.date) parts.push(display.value.date)
  }
  if (display.value.worksNum) parts.push(`${Math.round(display.value.worksNum)} 首`)
  if (props.kind === 'sheet' && display.value.playCount) {
    parts.push(`${Math.round(display.value.playCount)} 播放`)
  }
  if (props.kind === 'artist' && !display.value.worksNum) parts.push('歌手作品')
  return parts.join(' · ') || props.platform
})

const emptyText = computed(() => {
  if (props.kind === 'artist') return workType.value === 'music' ? '暂无单曲' : '暂无专辑'
  if (props.kind === 'sheet') return '歌单暂无歌曲'
  if (props.kind === 'toplist') return '榜单暂无歌曲'
  return '专辑暂无歌曲'
})

async function load(reset = true) {
  if (reset) {
    page.value = 1
    detail.value = null
  }
  loading.value = true
  error.value = ''
  try {
    const result = await getMusicCollectionDetail({
      platform: props.platform,
      kind: props.kind,
      item: props.item,
      page: page.value,
      workType: workType.value,
    })
    if (reset || !detail.value) {
      detail.value = result
    } else {
      detail.value = {
        item: detail.value.item ?? result.item,
        items: [...detail.value.items, ...result.items],
        collections: [...detail.value.collections, ...result.collections],
        isEnd: result.isEnd,
      }
    }
    hasMore.value = !result.isEnd
    if (!result.isEnd) page.value += 1
  } catch (e) {
    error.value = (e as Error).message || '加载详情失败'
    hasMore.value = false
  } finally {
    loading.value = false
  }
}

function switchWorkType(type: 'music' | 'album') {
  if (workType.value === type) return
  workType.value = type
  void load(true)
}

function playAll() {
  if (listItems.value.length) emit('play', listItems.value[0], listItems.value)
}

watch(
  () => `${props.platform}:${props.kind}:${props.item.id}`,
  () => {
    workType.value = 'music'
    void load(true)
  },
)

onMounted(() => void load(true))
</script>

<style scoped>
.detail-panel {
  position: absolute;
  inset: 0;
  z-index: 3;
  display: flex;
  flex-direction: column;
  background: var(--color-bg);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
}

.detail-head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-divider);
  flex-shrink: 0;
}

.back-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  flex-shrink: 0;
  border-radius: var(--radius-md);
  color: var(--color-text-secondary);
}

.back-btn:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.back-btn svg {
  width: 18px;
  height: 18px;
}

.detail-cover {
  width: 56px;
  height: 56px;
  flex-shrink: 0;
  border-radius: var(--radius-md);
  object-fit: cover;
  background: var(--color-bg-sunken);
}

.detail-cover.placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
  font-size: 20px;
  font-weight: 600;
}

.detail-copy {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.detail-copy h2 {
  margin: 0;
  color: var(--color-text);
  font-size: var(--text-lg);
  font-weight: 700;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.detail-copy p {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.detail-copy .detail-desc {
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.play-all {
  flex-shrink: 0;
  padding: 7px 16px;
  border-radius: var(--radius-full);
  background: var(--color-primary-soft-bg);
  color: var(--color-primary-soft-text);
  font-size: 12px;
  font-weight: 600;
}

.detail-tabs {
  display: flex;
  gap: 2px;
  padding: 3px;
  margin: var(--space-3) var(--space-4) 0;
  align-self: flex-start;
  border-radius: var(--radius-full);
  background: var(--color-bg-sunken);
  flex-shrink: 0;
}

.detail-tabs button {
  padding: 4px 16px;
  border-radius: var(--radius-full);
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.detail-tabs button.active {
  background: var(--color-bg-elevated);
  color: var(--color-primary);
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.detail-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4) var(--space-4);
  overflow-y: auto;
}

.detail-state {
  margin: var(--space-5) 0;
  color: var(--color-text-tertiary);
  font-size: 12px;
  text-align: center;
}

.row-action {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
}

.row-action svg {
  width: 15px;
  height: 15px;
}

.row-action:hover {
  background: var(--color-bg-sunken);
  color: var(--color-primary);
}

.more-row {
  display: flex;
  justify-content: center;
  padding: var(--space-2) 0;
}

.state-btn {
  padding: 6px 16px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.state-btn:hover:not(:disabled) {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.state-btn:disabled {
  opacity: 0.55;
  cursor: wait;
}
</style>
