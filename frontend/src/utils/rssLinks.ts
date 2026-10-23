/**
 * RSS 正文链接的站内处理。
 *
 * 正文是 `v-html` 注入的, 里面的 `<a>` 若放任默认导航, WebView2 会交给 Tauri 的
 * `on_navigation`, 非白名单域名一律丢给系统浏览器 —— 这正是"点查看全文跳浏览器"
 * 的来源。RssView 与 RssArticleView 共用这里的两个函数。
 */

/** 正文里的 `href` 可能是相对地址(甚至只有一个路径), 按当前文章链接补全。 */
export function resolveContentLink(raw: string | null, base: string): string {
  const value = (raw || '').trim()
  if (!value || value.startsWith('#') || /^javascript:/i.test(value)) return ''
  if (/^https?:\/\//i.test(value)) return value
  try {
    return new URL(value, base).toString()
  } catch {
    return ''
  }
}

export interface ContentClickLink {
  href: string
  title?: string
}

/**
 * 从点击事件里取出要站内打开的链接, 并当场拦下默认导航。
 *
 * 窗口级捕获监听与容器上的 `@click` 都会进来, 用 `defaultPrevented` 保证只处理一次。
 * 返回 `null` 表示这次点击与正文链接无关(或链接不可用), 调用方直接忽略。
 */
export function resolveContentClickLink(event: MouseEvent, base: string): ContentClickLink | null {
  if (event.defaultPrevented) return null
  const anchor = (event.target as HTMLElement | null)?.closest('a')
  if (!anchor) return null
  // 先拦下默认导航, 避免 WebView2 把它交给系统浏览器打开。
  event.preventDefault()
  event.stopPropagation()
  const href = resolveContentLink(anchor.getAttribute('href'), base)
  if (!href) return null
  return { href, title: (anchor.textContent || '').trim() || undefined }
}
