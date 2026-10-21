<template>
  <div class="discover-pane">
    <div v-if="!store.enabledPlugins.length" class="music-state">
      <p>还没有启用的音乐插件</p>
      <small>安装并启用 MusicFree 插件后，这里会显示榜单与推荐歌单。</small>
    </div>

    <template v-else>
      <div class="platform-chips">
        <button
          v-for="plugin in store.enabledPlugins"
          :key="plugin.platform"
          type="button"
          class="platform-chip"
          :class="{ active: store.activePlatform === plugin.platform }"
          :title="plugin.name"
          @click="store.setActivePlatform(plugin.platform)"
        >{{ plugin.name }}</button>
      </div>

      <section class="discover-section">
        <header class="section-head">
          <h2>推荐歌单</h2>
          <span v-if="activeTag" class="section-meta">{{ activeTag.title }}</span>
        </header>
        <p v-if="loadingTags && !tags" class="section-state">正在加载推荐歌单…</p>
        <p v-else-if="errorTags" class="section-state">{{ errorTags }}</p>
        <template v-else-if="tags">
          <MusicCollectionList
            v-if="tags.pinned.length"
            :items="tags.pinned"
            empty-text=""
            @open="emit('open', $event, 'sheet')"
          ></MusicCollectionList>
          <div v-for="group in tags.groups" :key="group.title" class="tag-group">
            <h3 v-if="group.title">{{ group.title }}</h3>
            <div class="tag-chips">
              <button
                v-for="tag in group.tags"
                :key="tag.id"
                type="button"
                class="tag-chip"
                :class="{ active: activeTag?.id === tag.id }"
                @click="selectTag(tag)"
              >{{ tag.title }}</button>
            </div>
          </div>
          <template v-if="activeTag">
            <p v-if="loadingSheets && !sheets.length" class="section-state">正在加载歌单…</p>
            <p v-else-if="errorSheets" class="section-state">{{ errorSheets }}</p>
            <MusicCollectionList
              v-else
              :items="sheets"
              empty-text="该分类下暂无歌单"
              @open="emit('open', $event, 'sheet')"
            ></MusicCollectionList>
            <div v-if="sheetHasMore" class="more-row">
              <button type="button" class="state-btn" :disabled="loadingSheets" @click="loadSheets(false)">
                {{ loadingSheets ? '加载中…' : '加载更多' }}
              </button>
            </div>
          </template>
        </template>
      </section>

      <section class="discover-section">
        <header class="section-head">
          <h2>榜单</h2>
        </header>
        <p v-if="loadingTop && !topGroups.length" class="section-state">正在加载榜单…</p>
        <p v-else-if="errorTop" class="section-state">{{ errorTop }}</p>
        <div v-for="group in topGroups" :key="group.title" class="top-group">
          <h3 v-if="group.title">{{ group.title }}</h3>
          <div class="top-row">
            <button
              v-for="item in group.items"
              :key="`${item.platform}:${item.id}`"
              type="button"
              class="top-chip"
              :title="item.title"
              @click="emit('open', item, 'toplist')"
            >
              <img
                v-if="item.artwork"
                class="top-cover"
                :src="item.artwork"
                :alt="item.title"
                loading="lazy"
                referrerpolicy="no-referrer"
              />
              <span v-else class="top-cover placeholder">{{ (item.title || '?').slice(0, 1) }}</span>
              <span class="top-title">{{ item.title }}</span>
            </button>
          </div>
        </div>
      </section>
    </template>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import {
  getMusicRecommendSheets,
  getMusicRecommendTags,
  getMusicTopLists,
  type MusicCollectionItem,
  type MusicCollectionKind,
  type MusicRecommendTags,
  type MusicSheetGroup,
  type MusicTag,
} from '../../api/music'
import { useMusicStore } from '../../stores/music'
import MusicCollectionList from './MusicCollectionList.vue'

const emit = defineEmits<{
  open: [item: MusicCollectionItem, kind: MusicCollectionKind]
}>()

const store = useMusicStore()

const topGroups = ref<MusicSheetGroup[]>([])
const tags = ref<MusicRecommendTags | null>(null)
const activeTag = ref<MusicTag | null>(null)
const sheets = ref<MusicCollectionItem[]>([])
const sheetPage = ref(1)
const sheetHasMore = ref(false)
const loadingTop = ref(false)
const loadingTags = ref(false)
const loadingSheets = ref(false)
const errorTop = ref('')
const errorTags = ref('')
const errorSheets = ref('')

/** 插件没实现某个能力时给出可读提示，而不是原始的 JS 报错。 */
function friendlyError(error: unknown) {
  const message = (error as Error).message || '加载失败'
  if (message.includes('未实现')) return '该音源插件不支持这个功能'
  return message
}

function reset() {
  topGroups.value = []
  tags.value = null
  activeTag.value = null
  sheets.value = []
  sheetPage.value = 1
  sheetHasMore.value = false
  errorTop.value = ''
  errorTags.value = ''
  errorSheets.value = ''
}

async function loadDiscover() {
  const platform = store.activePlatform
  reset()
  if (!platform) return
  loadingTop.value = true
  loadingTags.value = true
  const [topResult, tagResult] = await Promise.allSettled([
    getMusicTopLists(platform),
    getMusicRecommendTags(platform),
  ])
  if (topResult.status === 'fulfilled') topGroups.value = topResult.value
  else errorTop.value = friendlyError(topResult.reason)
  if (tagResult.status === 'fulfilled') tags.value = tagResult.value
  else errorTags.value = friendlyError(tagResult.reason)
  loadingTop.value = false
  loadingTags.value = false

  const firstTag = tags.value?.groups.flatMap((group) => group.tags)[0]
  if (firstTag) void selectTag(firstTag)
}

async function selectTag(tag: MusicTag) {
  activeTag.value = tag
  sheets.value = []
  sheetPage.value = 1
  sheetHasMore.value = false
  await loadSheets(true)
}

async function loadSheets(isReset: boolean) {
  const platform = store.activePlatform
  if (!platform || !activeTag.value) return
  if (isReset) {
    sheets.value = []
    sheetPage.value = 1
    sheetHasMore.value = false
  }
  loadingSheets.value = true
  errorSheets.value = ''
  try {
    const result = await getMusicRecommendSheets({
      platform,
      tag: activeTag.value,
      page: sheetPage.value,
    })
    sheets.value = isReset ? result.items : [...sheets.value, ...result.items]
    sheetHasMore.value = !result.isEnd && result.items.length > 0
    if (!result.isEnd) sheetPage.value += 1
  } catch (error) {
    errorSheets.value = friendlyError(error)
  } finally {
    loadingSheets.value = false
  }
}

watch(
  () => store.activePlatform,
  () => void loadDiscover(),
)

onMounted(() => void loadDiscover())
</script>

<style scoped>
.discover-pane {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  overflow-y: auto;
  padding-bottom: var(--space-2);
}

.platform-chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.platform-chip {
  padding: 5px 12px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.platform-chip.active {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.discover-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.section-head {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
}

.section-head h2 {
  margin: 0;
  color: var(--color-text);
  font-size: var(--text-base);
  font-weight: 700;
}

.section-meta {
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.section-state {
  margin: var(--space-2) 0;
  color: var(--color-text-tertiary);
  font-size: 12px;
}

.tag-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.tag-group h3,
.top-group h3 {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: 11px;
  font-weight: 600;
}

.tag-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.tag-chip {
  padding: 4px 12px;
  border-radius: var(--radius-full);
  border: 1px solid var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 12px;
}

.tag-chip:hover {
  border-color: var(--color-primary-border);
  color: var(--color-primary);
}

.tag-chip.active {
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.top-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.top-row {
  display: flex;
  gap: var(--space-3);
  overflow-x: auto;
  padding-bottom: 4px;
}

.top-chip {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 92px;
  flex-shrink: 0;
  text-align: left;
}

.top-cover {
  width: 92px;
  height: 92px;
  border-radius: var(--radius-md);
  object-fit: cover;
  background: var(--color-bg-sunken);
}

.top-cover.placeholder {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
  font-size: 22px;
  font-weight: 600;
}

.top-title {
  color: var(--color-text-secondary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.top-chip:hover .top-title {
  color: var(--color-primary);
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
