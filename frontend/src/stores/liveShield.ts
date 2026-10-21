import { defineStore } from 'pinia'
import { ref } from 'vue'

/**
 * 弹幕屏蔽词：纯 localStorage 持久化，与 simple_live 的屏蔽词一致。
 * 普通词按包含匹配弹幕内容/用户名；`/pattern/` 形式按正则匹配（忽略大小写）。
 */
const SHIELD_WORDS_KEY = 'live-danmaku-shield-words'

function loadWords(): string[] {
  try {
    const raw = localStorage.getItem(SHIELD_WORDS_KEY)
    const list = raw ? JSON.parse(raw) : []
    if (!Array.isArray(list)) return []
    return list.filter((item): item is string => typeof item === 'string' && item.trim() !== '')
  } catch {
    return []
  }
}

/** `...` 按包含匹配；`/.../` 按正则匹配（非法正则回退为包含匹配）。 */
export function matchShieldWord(word: string, text: string): boolean {
  if (word.length > 2 && word.startsWith('/') && word.endsWith('/')) {
    try {
      return new RegExp(word.slice(1, -1), 'i').test(text)
    } catch {
      // 非法正则当普通词处理，避免一个坏词让整条匹配抛错。
    }
  }
  return text.includes(word)
}

export const useLiveShieldStore = defineStore('liveShield', () => {
  const words = ref<string[]>(loadWords())

  function persist() {
    localStorage.setItem(SHIELD_WORDS_KEY, JSON.stringify(words.value))
  }

  function addWord(word: string) {
    const trimmed = word.trim()
    if (!trimmed || words.value.includes(trimmed)) return
    words.value = [...words.value, trimmed]
    persist()
  }

  function removeWord(word: string) {
    words.value = words.value.filter((item) => item !== word)
    persist()
  }

  function clearWords() {
    words.value = []
    persist()
  }

  /** 弹幕是否应被屏蔽：匹配内容或用户名任一即屏蔽。 */
  function shouldBlock(content: string, user = ''): boolean {
    return words.value.some((word) => matchShieldWord(word, content) || matchShieldWord(word, user))
  }

  return { words, addWord, removeWord, clearWords, shouldBlock }
})
