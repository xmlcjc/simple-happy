/**
 * 直播间链接解析：粘贴平台直播间 URL，自动识别平台与房间号后直接跳房。
 * 各平台房间号形态：
 * - B 站 / 斗鱼 / 虎牙：纯数字；
 * - 抖音：webRid（字母数字混合，走 live.douyin.com 域名限定，避免误吞用户主页）。
 */

export interface ParsedLiveLink {
  platform: string
  roomId: string
}

const LINK_RULES: Array<{ platform: string; pattern: RegExp }> = [
  { platform: 'bilibili', pattern: /bilibili\.com\/(?:bl\/)?(\d+)/i },
  { platform: 'douyu', pattern: /douyu\.com\/(\d+)/i },
  { platform: 'huya', pattern: /huya\.com\/(\d+)/i },
  { platform: 'douyin', pattern: /live\.douyin\.com\/([0-9A-Za-z]+)/ },
]

/** 解析一段文本里的直播间链接；不是可识别的直播间地址时返回 null。 */
export function parseLiveLink(text: string): ParsedLiveLink | null {
  const trimmed = text.trim()
  if (!trimmed) return null
  for (const rule of LINK_RULES) {
    const matched = rule.pattern.exec(trimmed)
    if (matched?.[1]) {
      return { platform: rule.platform, roomId: matched[1] }
    }
  }
  return null
}
