import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { deleteRssSource, deleteRssSources, getRssArticles, getRssContent, getRssSources, saveRssSources } from '../api/rss'
import type { RssArticle, RssSource } from '../types'
import { saveRecentReadBook } from '../utils/recentBooks'
import { recordRssRead } from '../utils/rssReadHistory'
import { toRssSourceDeletePayload } from '../utils/sourceSelection'

type ArticleScope = 'source' | 'group' | 'all'
type ScopedArticle = RssArticle & {
  variable?: string
  /** 该条目是"单 URL 源"的网页入口(源地址本身就是一篇文章), 而非真 feed 文章。 */
  singleUrlEntry?: boolean
}

export const useRssStore = defineStore('rss', () => {
  const sources = ref<RssSource[]>([])
  const activeSourceUrl = ref('')
  const articleScope = ref<ArticleScope>('source')
  const activeGroupName = ref('')
  const articles = ref<ScopedArticle[]>([])
  const page = ref(1)
  const hasMore = ref(true)
  const loading = ref(false)
  const contentLoading = ref(false)
  const activeArticle = ref<ScopedArticle | null>(null)
  const activeContent = ref('')
  /** 当前范围是否包含"单 URL 源"(源地址即一篇文章, 点开看网页而非文章列表)。 */
  const singleUrlMode = ref(false)
  /** 单 URL 源的地址是否由源里的 JS 求值得到(自动选到可用节点)。 */
  const singleUrlResolved = ref(false)

  const activeSource = computed(() => sources.value.find((item) => item.sourceUrl === activeSourceUrl.value) || null)
  const enabledSources = computed(() => sources.value.filter((source) => source.enabled !== false))
  const groupNames = computed(() => {
    const groups = new Set<string>()
    enabledSources.value.forEach((source) => {
      ;(source.sourceGroup || '')
        .split(/[,;，、]/)
        .map((item) => item.trim())
        .filter(Boolean)
        .forEach((item) => groups.add(item))
    })
    return Array.from(groups).sort()
  })

  function normalizeRssContent(content?: string | null, description?: string | null) {
    const raw = (content || description || '').trim()
    if (!raw) return ''

    const lowered = raw.toLowerCase()
    if (lowered.includes('<html') || lowered.includes('<body') || lowered.includes('<head')) {
      return ''
    }

    return raw
      .replace(/<script[\s\S]*?<\/script>/gi, '')
      .replace(/<style[\s\S]*?<\/style>/gi, '')
      .replace(/<noscript[\s\S]*?<\/noscript>/gi, '')
      .trim()
  }

  function normalizeFetchedContent(content?: string | null) {
    const raw = (content || '').trim()
    if (!raw) return ''

    // 后端现在只返回正文容器(已去掉导航/脚本/广告), 这里不再按"整页"启发式
    // 丢弃 —— 正文里出现"首页/搜索"字样是正常的。
    return raw
      .replace(/<script[\s\S]*?<\/script>/gi, '')
      .replace(/<style[\s\S]*?<\/style>/gi, '')
      .replace(/<noscript[\s\S]*?<\/noscript>/gi, '')
      .trim()
  }

  /** 正文是否是"摘要/被截断"(只有摘要或带"查看全文"链接)。
   *
   *  判定不再只看字数与固定文案: 只要正文里的链接指回文章自身, 就说明订阅源
   *  只给了摘要 + 一个「查看全文」入口(各家文案不同: 阅读全文 / 查看原文 /
   *  Read more / 全文 »), 一律视为截断并去抓原网页。
   */
  function isTruncatedContent(content?: string | null, articleLink?: string | null) {
    const raw = (content || '').trim()
    if (!raw) return true
    const text = raw
      .replace(/<script[\s\S]*?<\/script>/gi, ' ')
      .replace(/<style[\s\S]*?<\/style>/gi, ' ')
      .replace(/<[^>]+>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim()
    if (!text) return true
    if (text.length < 400) return true
    if (
      /(?:阅读|查看|继续|点击|显示)\s*(?:全文|原文)|read\s*more|continue\s*reading|full\s*(?:story|article)|全文\s*[»›>]/i.test(
        text,
      )
    ) {
      return true
    }
    return containsSelfLink(raw, articleLink)
  }

  /** 正文里是否有一个链接指回文章自身(feed 的「查看全文」通常就是这么写的)。 */
  function containsSelfLink(html: string, articleLink?: string | null) {
    if (!articleLink) return false
    const self = comparableUrl(articleLink)
    if (!self) return false
    for (const match of html.matchAll(/href\s*=\s*["']([^"']+)["']/gi)) {
      const href = match[1]
      if (!href || href.startsWith('#')) continue
      if (comparableUrl(href) === self) return true
    }
    return false
  }

  /** 去掉协议/末尾斜杠/查询串, 只比对「主机 + 路径」, 相对地址按原样比较路径。 */
  function comparableUrl(value: string) {
    const raw = value.trim()
    if (!raw) return ''
    try {
      const parsed = new URL(raw)
      return `${parsed.host}${parsed.pathname.replace(/\/+$/, '')}`.toLowerCase()
    } catch {
      const path = raw.split(/[?#]/)[0] || ''
      return path.replace(/^\/+|\/+$/g, '').toLowerCase()
    }
  }

  function sourceGroups(source: RssSource) {
    return (source.sourceGroup || '')
      .split(/[,;，、]/)
      .map((item) => item.trim())
      .filter(Boolean)
  }

  function resolveScopeSources() {
    if (articleScope.value === 'all') return enabledSources.value
    if (articleScope.value === 'group' && activeGroupName.value) {
      return enabledSources.value.filter((source) => sourceGroups(source).includes(activeGroupName.value))
    }
    return enabledSources.value.filter((source) => source.sourceUrl === activeSourceUrl.value)
  }

  async function fetchSources() {
    sources.value = await getRssSources().catch(() => [])
    if (!activeSourceUrl.value && enabledSources.value.length) {
      activeSourceUrl.value = enabledSources.value[0]!.sourceUrl
    }
  }

  async function addSources(list: RssSource[]) {
    await saveRssSources(list)
    await fetchSources()
  }

  async function removeSource(source: RssSource) {
    await deleteRssSource({ sourceUrl: source.sourceUrl, sourceName: source.sourceName })
    if (activeSourceUrl.value === source.sourceUrl) {
      activeSourceUrl.value = ''
      articles.value = []
      activeArticle.value = null
      activeContent.value = ''
    }
    await fetchSources()
  }

  async function removeSources(targets: RssSource[]) {
    if (!targets.length) return 0
    const targetUrls = new Set(targets.map((source) => source.sourceUrl))
    const result = await deleteRssSources(toRssSourceDeletePayload(targets))
    if (targetUrls.has(activeSourceUrl.value)) {
      activeSourceUrl.value = ''
      articles.value = []
      activeArticle.value = null
      activeContent.value = ''
    }
    await fetchSources()
    return result.deleted
  }

  async function fetchArticlesForSource(source: RssSource, pageNo: number) {
    const res = await getRssArticles({
      sourceUrl: source.sourceUrl,
      sortName: '默认',
      sortUrl: source.sortUrl || source.sourceUrl,
      page: pageNo,
    })
    // `singleUrl` 源没有文章列表, 后端只回一条合成条目; 标记它以便逐条区分,
    // 避免"范围内有单 URL 源"就把真 feed 的文章也当成网页入口(会丢列表/数量/加载更多)。
    const singleUrl = res.singleUrl === true
    return {
      articles: (res.first || []).map((article) => ({
        ...article,
        variable: source.sourceUrl,
        origin: source.sourceName || article.origin,
        singleUrlEntry: singleUrl || undefined,
      })),
      singleUrl,
      urlEvaluated: res.urlEvaluated === true,
    }
  }

  async function fetchArticles(reset = false) {
    if (loading.value) return
    if (!enabledSources.value.length) return
    if (articleScope.value === 'source' && !activeSourceUrl.value) return

    if (reset) {
      page.value = 1
      hasMore.value = true
      articles.value = []
    }

    loading.value = true
    try {
      const targets = resolveScopeSources()
      const results = await Promise.all(
        targets.map((source) => fetchArticlesForSource(source, page.value)),
      )
      const chunks = results.map((result) => result.articles)
      // `singleUrlMode` 只表示"**当前范围里每一条都是网页入口**", 用于决定列表标题等
      // 整体呈现。混合范围(真 feed + 单 URL 源)必须按条目判断, 否则真 feed 的文章
      // 会被连坐成网页入口。
      const allSingleUrl = results.length > 0 && results.every((result) => result.singleUrl)
      singleUrlMode.value = allSingleUrl
      singleUrlResolved.value = results.some((result) => result.singleUrl && result.urlEvaluated)
      const list = chunks
        .flat()
        .sort((a, b) => {
          const timeA = Date.parse(a.pubDate || '') || a.order || 0
          const timeB = Date.parse(b.pubDate || '') || b.order || 0
          return timeB - timeA
        })

      articles.value = reset ? list : articles.value.concat(list)
      // 单 URL 源没有"下一页", 它的合成条目恒为 1 条, 不能据此认为还有更多。
      hasMore.value = chunks.some(
        (chunk) => chunk.length >= 50 && !chunk.every((item) => item.singleUrlEntry),
      )

      if (list.length) {
        page.value += 1
      }
    } finally {
      loading.value = false
    }
  }

  /** 阅读区内的"链接跳转"历史: 支持从站内打开的页面退回上一篇。 */
  const contentHistory = ref<ScopedArticle[]>([])

  async function openArticle(
    article: ScopedArticle,
    options: { keepHistory?: boolean; skipRecord?: boolean } = {},
  ) {
    const sourceUrl = article.variable || activeSourceUrl.value
    if (!sourceUrl) return
    if (
      !options.keepHistory &&
      activeArticle.value &&
      (activeArticle.value.link !== article.link ||
        activeArticle.value.variable !== article.variable)
    ) {
      contentHistory.value = [...contentHistory.value, activeArticle.value].slice(-20)
    }
    activeArticle.value = article
    const inlineContent = normalizeRssContent(article.content, article.description)
    activeContent.value = inlineContent
    contentLoading.value = false

    if (!options.skipRecord) {
      saveRecentReadBook({
        name: article.title || 'RSS 文章',
        author: article.origin || 'RSS',
        bookUrl: article.link,
        origin: `rss:${sourceUrl}`,
        intro: article.description,
        durChapterTime: Date.now(),
        recentKind: 'rss',
        rssSourceUrl: sourceUrl,
        rssLink: article.link,
        rssPubDate: article.pubDate,
      })
      recordRssRead({
        title: article.title || 'RSS 文章',
        sourceUrl,
        link: article.link,
        pubDate: article.pubDate,
      })
    }

    // 订阅源只给了 description(没有 content:encoded), 或正文是"摘要 + 查看全文"
    // 时, 一律去抓原网页全文; 只有内嵌正文足够长且没有指回自身的链接才跳过。
    const hasFeedContent = Boolean((article.content || '').trim())
    if (hasFeedContent && !isTruncatedContent(inlineContent, article.link)) return
    contentLoading.value = !inlineContent
    try {
      const fetchedContent = normalizeFetchedContent(
        await getRssContent({
          sourceUrl,
          link: article.link,
          origin: article.origin || sourceUrl,
        }),
      )
      // 抓到的全文才替换; 抓取失败时保留原来的摘要, 不让阅读区变空。
      if (fetchedContent && activeArticle.value?.link === article.link) {
        activeContent.value = fetchedContent
      }
    } catch {
      // 抓取失败保留内嵌摘要。
    } finally {
      contentLoading.value = false
    }
  }

  /** 重新抓取当前文章的全文(手动兜底)。 */
  async function refetchContent() {
    const article = activeArticle.value
    if (!article) return
    const sourceUrl = article.variable || activeSourceUrl.value
    if (!sourceUrl) return
    contentLoading.value = true
    try {
      const fetchedContent = normalizeFetchedContent(
        await getRssContent({
          sourceUrl,
          link: article.link,
          origin: article.origin || sourceUrl,
        }),
      )
      if (fetchedContent) activeContent.value = fetchedContent
    } finally {
      contentLoading.value = false
    }
  }

  /** 正文里的链接: 在应用内阅读区打开, 绝不交给系统浏览器。 */
  async function openLinkedPage(url: string, title?: string) {
    if (!/^https?:\/\//i.test(url)) return
    const sourceUrl = activeSourceUrl.value
    if (!sourceUrl) return
    const article: ScopedArticle = {
      origin: activeArticle.value?.origin || '链接',
      sort: '',
      title: title || url,
      order: 0,
      link: url,
      pubDate: '',
      variable: sourceUrl,
    }
    await openArticle(article, { skipRecord: true })
  }

  /** 返回上一篇(从正文里的链接退回)。 */
  async function backToPreviousArticle() {
    const previous = contentHistory.value[contentHistory.value.length - 1]
    if (!previous) return
    contentHistory.value = contentHistory.value.slice(0, -1)
    await openArticle(previous, { keepHistory: true, skipRecord: true })
  }

  /** 清空"链接跳转"历史(切换源/范围时)。 */
  function clearContentHistory() {
    contentHistory.value = []
  }

  async function setSource(url: string) {
    activeSourceUrl.value = url
    articleScope.value = 'source'
    activeGroupName.value = ''
    activeArticle.value = null
    activeContent.value = ''
    contentHistory.value = []
    await fetchArticles(true)
  }

  async function setGroup(name: string) {
    articleScope.value = 'group'
    activeGroupName.value = name
    activeArticle.value = null
    activeContent.value = ''
    contentHistory.value = []
    await fetchArticles(true)
  }

  async function setAllSources() {
    articleScope.value = 'all'
    activeGroupName.value = ''
    activeArticle.value = null
    activeContent.value = ''
    contentHistory.value = []
    await fetchArticles(true)
  }

  return {
    sources,
    activeSourceUrl,
    activeSource,
    enabledSources,
    groupNames,
    articleScope,
    activeGroupName,
    articles,
    page,
    hasMore,
    loading,
    contentLoading,
    activeArticle,
    activeContent,
    contentHistory,
    singleUrlMode,
    singleUrlResolved,
    fetchSources,
    addSources,
    removeSource,
    removeSources,
    fetchArticles,
    openArticle,
    openLinkedPage,
    backToPreviousArticle,
    refetchContent,
    clearContentHistory,
    setSource,
    setGroup,
    setAllSources,
  }
})
