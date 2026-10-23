import { invokeEnvelope } from './invoke'

/** 本地划线：书 + 章 + 段落序号，完全保存在设备上。 */
export interface BookHighlight {
  bookUrl: string
  bookName: string
  bookAuthor: string
  chapterIndex: number
  chapterUrl: string
  chapterTitle: string
  blockIndex: number
  text: string
  note: string
  createdAt: number
}

export function getHighlights(): Promise<BookHighlight[]> {
  return invokeEnvelope<BookHighlight[]>('get_highlights', {})
}

export function saveHighlight(highlight: BookHighlight): Promise<string> {
  return invokeEnvelope<string>('save_highlight', { req: highlight })
}

export function deleteHighlight(highlight: BookHighlight): Promise<string> {
  return invokeEnvelope<string>('delete_highlight', { req: highlight })
}

/** 批量合并（恢复备份的「合并」模式）。 */
export function saveHighlights(highlights: BookHighlight[]): Promise<string> {
  return invokeEnvelope<string>('save_highlights', { req: highlights })
}

/** 整表覆盖（恢复备份的「覆盖」模式）。 */
export function replaceHighlights(highlights: BookHighlight[]): Promise<string> {
  return invokeEnvelope<string>('replace_highlights', { req: highlights })
}

export function deleteBookHighlights(bookUrl: string): Promise<{ removed: number }> {
  return invokeEnvelope<{ removed: number }>('delete_book_highlights', { bookUrl })
}
