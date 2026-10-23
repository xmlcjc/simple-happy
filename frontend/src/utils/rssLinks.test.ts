import { describe, expect, it } from 'vitest'
import { resolveContentClickLink, resolveContentLink } from './rssLinks'

describe('resolveContentLink', () => {
  const BASE = 'https://site.example.com/a/1'

  it('keeps absolute http(s) links as-is', () => {
    expect(resolveContentLink('https://other.example.com/x', BASE)).toBe('https://other.example.com/x')
    expect(resolveContentLink('  HTTP://other.example.com/x  ', BASE)).toBe(
      'HTTP://other.example.com/x',
    )
  })

  it('resolves relative paths against the article link', () => {
    expect(resolveContentLink('/a/2', BASE)).toBe('https://site.example.com/a/2')
    expect(resolveContentLink('2', BASE)).toBe('https://site.example.com/a/2')
  })

  it('rejects empty, hash-only and javascript links', () => {
    expect(resolveContentLink(null, BASE)).toBe('')
    expect(resolveContentLink('', BASE)).toBe('')
    expect(resolveContentLink('   ', BASE)).toBe('')
    expect(resolveContentLink('#section', BASE)).toBe('')
    expect(resolveContentLink('javascript:void(0)', BASE)).toBe('')
    expect(resolveContentLink('JavaScript:alert(1)', BASE)).toBe('')
  })

  it('returns empty when a relative link has no usable base', () => {
    // 没有 activeArticle 时 base 是空串, 相对地址无法补全。
    expect(resolveContentLink('/a/2', '')).toBe('')
  })
})

describe('resolveContentClickLink', () => {
  const BASE = 'https://site.example.com/a/1'

  it('returns the resolved link and anchor text for a real content link', () => {
    const anchor = fakeAnchor('/a/2', '继续阅读')
    const { event, calls } = fakeEvent(fakeTarget(anchor))

    expect(resolveContentClickLink(event, BASE)).toEqual({
      href: 'https://site.example.com/a/2',
      title: '继续阅读',
    })
    expect(calls.preventDefault).toBe(1)
    expect(calls.stopPropagation).toBe(1)
  })

  it('omits the title when the anchor has no text', () => {
    const anchor = fakeAnchor('https://other.example.com/x', '   ')
    const { event } = fakeEvent(fakeTarget(anchor))

    expect(resolveContentClickLink(event, BASE)).toEqual({
      href: 'https://other.example.com/x',
      title: undefined,
    })
  })

  it('ignores clicks without an anchor and does not prevent default', () => {
    const { event, calls } = fakeEvent(fakeTarget(null))

    expect(resolveContentClickLink(event, BASE)).toBeNull()
    expect(calls.preventDefault).toBe(0)
    expect(calls.stopPropagation).toBe(0)
  })

  it('processes each click only once (capture listener + container @click)', () => {
    const { event, calls } = fakeEvent(fakeTarget(fakeAnchor('/a/2', 'x')), {
      defaultPrevented: true,
    })

    expect(resolveContentClickLink(event, BASE)).toBeNull()
    expect(calls.preventDefault).toBe(0)
  })

  it('still blocks navigation when the href itself is unusable', () => {
    // 链接不可用时返回 null, 但默认导航必须已经被拦下。
    const { event, calls } = fakeEvent(fakeTarget(fakeAnchor('#anchor', '锚点')))

    expect(resolveContentClickLink(event, BASE)).toBeNull()
    expect(calls.preventDefault).toBe(1)
    expect(calls.stopPropagation).toBe(1)
  })
})

function fakeAnchor(href: string | null, textContent: string) {
  return {
    getAttribute: (name: string) => (name === 'href' ? href : null),
    textContent,
  }
}

function fakeTarget(anchor: ReturnType<typeof fakeAnchor> | null) {
  return {
    closest: (selector: string) => (selector === 'a' ? anchor : null),
  }
}

function fakeEvent(target: unknown, options: { defaultPrevented?: boolean } = {}) {
  const calls = { preventDefault: 0, stopPropagation: 0 }
  const event = {
    defaultPrevented: options.defaultPrevented ?? false,
    target,
    preventDefault: () => {
      calls.preventDefault += 1
    },
    stopPropagation: () => {
      calls.stopPropagation += 1
    },
  }
  return { event: event as unknown as MouseEvent, calls }
}
