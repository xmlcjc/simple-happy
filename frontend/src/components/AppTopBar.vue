<template>
  <header class="app-topbar">
    <div class="topbar-inner">
      <!-- 品牌(logo + 名称)已移到系统标题栏, 顶栏只保留导航 -->
      <nav class="topbar-nav" aria-label="主导航">
        <button
          v-for="item in navItems"
          :key="item.key"
          class="nav-item"
          :class="{ active: activeKey === item.key }"
          :title="item.label"
          @click="go(item.path)"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path v-for="(path, index) in item.paths" :key="index" :d="path" />
          </svg>
          <span>{{ item.label }}</span>
        </button>
      </nav>

      <div class="topbar-right">
        <button class="topbar-btn" title="切换主题" @click="toggleTheme">
          <svg v-if="theme === 'light'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
          </svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="4" />
            <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
          </svg>
        </button>

        <button class="topbar-btn" @click="openSettings" title="设置">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
        </button>
      </div>
    </div>
  </header>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useAppStore } from '../stores/app'

type NavKey = 'home' | 'rss' | 'live' | 'music'

const router = useRouter()
const route = useRoute()
const appStore = useAppStore()

const theme = computed(() => appStore.theme)

const allItems: Array<{ key: NavKey; label: string; path: string; paths: string[] }> = [
  {
    key: 'home',
    label: '小说',
    path: '/',
    paths: ['M4 5.5A2.5 2.5 0 0 1 6.5 3H20', 'M4 5.5V19a2 2 0 0 0 2 2h14', 'M8 7h8', 'M8 11h8', 'M8 15h5'],
  },
  {
    key: 'rss',
    label: 'RSS',
    path: '/rss',
    paths: ['M4 11a9 9 0 0 1 9 9', 'M4 4a16 16 0 0 1 16 16', 'M5 19h.01'],
  },
  {
    key: 'music',
    label: '音乐',
    path: '/music',
    paths: [
      'M9 18V5l12-2v13',
      'M6 15a3 3 0 1 0 0 6a3 3 0 1 0 0-6z',
      'M18 13a3 3 0 1 0 0 6a3 3 0 1 0 0-6z',
    ],
  },
  {
    key: 'live',
    label: '直播',
    path: '/live',
    paths: ['M3 6h18v11H3z', 'M8 21h8', 'M12 17v4', 'M10 10.5v3l3-1.5'],
  },
]

// 隐藏功能开关沿用设置抽屉里的 'rss' / 'live' / 'music' 键。
const navItems = computed(() => {
  const hidden = (key: NavKey) => {
    if (key === 'rss') return appStore.isFeatureHidden('rss')
    if (key === 'live') return appStore.isFeatureHidden('live')
    if (key === 'music') return appStore.isFeatureHidden('music')
    return false
  }
  return allItems.filter((item) => !hidden(item.key))
})

const activeKey = computed<NavKey>(() => {
  if (route.path.startsWith('/live')) return 'live'
  if (route.path.startsWith('/music')) return 'music'
  if (route.path.startsWith('/rss')) return 'rss'
  return 'home'
})

function go(path: string) {
  if (route.path !== path) router.push(path)
}

function toggleTheme() {
  appStore.toggleTheme()
}

function openSettings() {
  appStore.showSettingsDrawer = true
}
</script>

<style scoped>
.app-topbar {
  position: sticky;
  top: 0;
  z-index: var(--z-sticky);
  min-height: calc(var(--header-height) + var(--safe-area-top));
  padding-top: var(--safe-area-top);
  background: color-mix(in srgb, var(--color-bg-elevated) 94%, transparent);
  border-bottom: 1px solid var(--color-divider);
  backdrop-filter: blur(18px) saturate(130%);
  -webkit-backdrop-filter: blur(18px) saturate(130%);
  box-sizing: border-box;
}

.topbar-inner {
  min-height: var(--header-height);
  /* 品牌移到标题栏后左侧留空, 用三栏网格保证导航始终居中 */
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  gap: var(--space-4);
  padding: 0 var(--space-6);
}

/* 主导航：与底栏同源的胶囊样式，横向排在中间 */
.topbar-nav {
  display: flex;
  align-items: center;
  gap: 4px;
  /* 网格自动排布会把导航放进第 1 列(1fr), 那样它只在左三分之一内居中;
   * 显式放到中间列, 左右各留一个 1fr 空列, 导航才相对整个窗口居中。 */
  grid-column: 2;
  justify-self: center;
  padding: 4px;
  border-radius: var(--radius-full);
  background: var(--color-bg);
  border: 1px solid var(--color-border-light);
  flex-shrink: 0;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border-radius: var(--radius-full);
  color: var(--color-text-secondary);
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
  transition: all var(--duration-fast) var(--ease-out);
}

.nav-item svg {
  width: 17px;
  height: 17px;
  flex-shrink: 0;
}

.nav-item:hover {
  color: var(--color-text);
  background: var(--color-bg-hover);
}

.nav-item.active {
  background: var(--color-primary-bg);
  border: 1px solid var(--color-primary-border);
  color: var(--color-primary);
  font-weight: 600;
}

.nav-item.active:hover {
  background: var(--color-primary-bg);
}

.topbar-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
  grid-column: 3;
  justify-self: end;
}

.topbar-btn {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 36px;
  min-height: 36px;
  padding: 8px;
  border-radius: var(--radius-lg);
  color: var(--color-text-secondary);
  transition: all var(--duration-fast) var(--ease-out);
}

.topbar-btn:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.topbar-btn:active {
  background: var(--color-bg-active);
  transform: scale(0.97);
}

.topbar-btn svg {
  width: 19px;
  height: 19px;
  flex-shrink: 0;
}

@media (max-width: 900px) {
  .nav-item span {
    display: none;
  }

  .nav-item {
    padding: 7px 11px;
  }
}

@media (max-width: 640px) {
  .topbar-inner {
    padding: 0 var(--space-3);
    gap: var(--space-2);
  }

  .topbar-nav {
    gap: 2px;
    padding: 3px;
  }

  .nav-item {
    padding: 6px 9px;
  }

  .topbar-btn {
    min-width: 34px;
    min-height: 34px;
    padding: 7px;
  }

  .topbar-btn svg {
    width: 18px;
    height: 18px;
  }
}
</style>
