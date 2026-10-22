import { createRouter, createWebHashHistory } from 'vue-router'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: '/',
      name: 'home',
      component: () => import('../views/HomeView.vue'),
    },
    {
      path: '/reader',
      name: 'reader',
      component: () => import('../views/ReaderView.vue'),
    },
    {
      path: '/recent',
      name: 'recent',
      redirect: '/?view=history',
    },
    {
      path: '/live',
      name: 'live',
      component: () => import('../views/LiveView.vue'),
    },
    {
      path: '/live/room',
      name: 'live-room',
      component: () => import('../views/LiveRoomView.vue'),
    },
    {
      path: '/music',
      name: 'music',
      component: () => import('../views/MusicView.vue'),
    },
    {
      path: '/rss',
      name: 'rss',
      component: () => import('../views/RssView.vue'),
    },
    {
      path: '/rss/manage',
      name: 'rss-manage',
      component: () => import('../views/RssManageView.vue'),
    },
    {
      path: '/rss/history',
      name: 'rss-history',
      component: () => import('../views/RssHistoryView.vue'),
    },
    {
      path: '/rss/article',
      name: 'rss-article',
      component: () => import('../views/RssArticleView.vue'),
    },
  ],
})

export default router
