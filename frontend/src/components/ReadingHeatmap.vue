<template>
  <section class="reading-heatmap" aria-label="阅读热力图">
    <header class="heatmap-header">
      <div>
        <h3>阅读热力图</h3>
        <p>最近 {{ weeks }} 周，每格代表一天</p>
      </div>
      <div class="heatmap-legend" aria-hidden="true">
        <span>少</span>
        <i v-for="level in 5" :key="level" :class="`level-${level - 1}`"></i>
        <span>多</span>
      </div>
    </header>

    <div class="heatmap-scroll">
      <div
        class="heatmap-months"
        :style="{ gridTemplateColumns: `26px repeat(${weeks}, 14px)` }"
      >
        <span
          v-for="month in months"
          :key="month.key"
          :style="{ gridColumn: `${month.column + 2}` }"
        >{{ month.label }}</span>
      </div>
      <div class="heatmap-body">
        <div class="heatmap-weekdays" aria-hidden="true">
          <span>一</span><span></span><span>三</span><span></span><span>五</span><span></span><span>日</span>
        </div>
        <div class="heatmap-grid">
          <template v-for="cell in cells" :key="cell.date">
            <span
              v-if="cell.future"
              class="heat-cell empty"
              :title="cell.date"
            ></span>
            <span
              v-else
              class="heat-cell"
              :class="`level-${cell.level}`"
              :title="`${cell.date} · ${cell.minutes} 分钟`"
            ></span>
          </template>
        </div>
      </div>
    </div>

    <p v-if="!hasData" class="heatmap-empty">继续阅读后，这里会按天记录阅读时长</p>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue'

export interface HeatmapDay {
  date: string
  seconds: number
}

const props = withDefaults(defineProps<{ daily: HeatmapDay[]; weeks?: number }>(), {
  weeks: 16,
})

const WEEKDAYS = 7
const DAY_MS = 24 * 60 * 60 * 1000

function formatDate(date: Date) {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

function startOfWeek(date: Date) {
  const copy = new Date(date.getFullYear(), date.getMonth(), date.getDate())
  const weekday = (copy.getDay() + 6) % 7 // 周一为 0
  copy.setDate(copy.getDate() - weekday)
  return copy
}

const secondsByDate = computed(() => {
  const map = new Map<string, number>()
  for (const item of props.daily) {
    map.set(item.date, (map.get(item.date) || 0) + (item.seconds || 0))
  }
  return map
})

function levelOf(minutes: number) {
  if (minutes <= 0) return 0
  if (minutes < 15) return 1
  if (minutes < 30) return 2
  if (minutes < 60) return 3
  return 4
}

const cells = computed(() => {
  const today = new Date()
  const todayKey = formatDate(today)
  const start = startOfWeek(today)
  start.setDate(start.getDate() - (props.weeks - 1) * WEEKDAYS)

  const result: Array<{ date: string; minutes: number; level: number; future: boolean }> = []
  for (let week = 0; week < props.weeks; week += 1) {
    for (let day = 0; day < WEEKDAYS; day += 1) {
      const date = new Date(start.getTime() + (week * WEEKDAYS + day) * DAY_MS)
      const key = formatDate(date)
      const future = key > todayKey
      const seconds = secondsByDate.value.get(key) || 0
      const minutes = Math.round(seconds / 60)
      result.push({ date: key, minutes, level: future ? 0 : levelOf(minutes), future })
    }
  }
  return result
})

const hasData = computed(() => props.daily.some((item) => item.seconds > 0))

/** 每个月第一次出现的那一列上方标出月份。 */
const months = computed(() => {
  const list: Array<{ key: string; label: string; column: number }> = []
  let lastMonth = ''
  for (let week = 0; week < props.weeks; week += 1) {
    const cell = cells.value[week * WEEKDAYS]
    if (!cell) continue
    const date = new Date(`${cell.date}T00:00:00`)
    const month = `${date.getFullYear()}-${date.getMonth()}`
    if (month !== lastMonth && date.getDate() <= 7) {
      list.push({ key: month, label: `${date.getMonth() + 1}月`, column: week })
      lastMonth = month
    }
  }
  return list
})
</script>

<style scoped>
.reading-heatmap {
  margin-bottom: var(--space-5, 20px);
  padding: var(--space-4, 16px);
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg, 12px);
}

.heatmap-header {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-3, 12px);
  margin-bottom: var(--space-3, 12px);
}

.heatmap-header h3 {
  margin: 0 0 2px;
  font-size: var(--text-sm, 14px);
  font-weight: 600;
  color: var(--color-text);
}

.heatmap-header p {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: var(--text-xs, 12px);
}

.heatmap-legend {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--color-text-tertiary);
  font-size: 11px;
}

.heatmap-legend i {
  display: block;
  width: 11px;
  height: 11px;
  border-radius: 3px;
}

.heatmap-scroll {
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-x: auto;
  padding-bottom: 2px;
}

.heatmap-months {
  display: grid;
  /* 星期标签列 + 每周一列；列宽由模板内联样式给出，与下方格子一致。 */
  gap: 3px;
  color: var(--color-text-tertiary);
  font-size: 10px;
  line-height: 1;
  min-width: max-content;
}

.heatmap-body {
  display: flex;
  gap: 3px;
  min-width: max-content;
}

.heatmap-weekdays {
  display: grid;
  grid-template-rows: repeat(7, 11px);
  gap: 3px;
  width: 23px;
  color: var(--color-text-tertiary);
  font-size: 10px;
  line-height: 11px;
  text-align: right;
}

.heatmap-grid {
  display: grid;
  grid-auto-flow: column;
  grid-template-rows: repeat(7, 11px);
  grid-auto-columns: 11px;
  gap: 3px;
}

.heat-cell {
  display: block;
  width: 11px;
  height: 11px;
  border-radius: 3px;
  background: var(--color-bg-hover);
}

.heat-cell.empty {
  background: transparent;
}

.level-0 { background: color-mix(in srgb, var(--color-border-light) 55%, transparent); }
.level-1 { background: color-mix(in srgb, var(--color-primary) 22%, transparent); }
.level-2 { background: color-mix(in srgb, var(--color-primary) 42%, transparent); }
.level-3 { background: color-mix(in srgb, var(--color-primary) 65%, transparent); }
.level-4 { background: var(--color-primary); }

.heatmap-empty {
  margin: var(--space-2, 8px) 0 0;
  color: var(--color-text-tertiary);
  font-size: var(--text-xs, 12px);
}
</style>
