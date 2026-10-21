import { describe, expect, it } from 'vitest'
import { parseLiveLink } from './liveLink'

describe('parseLiveLink', () => {
  it('parses plain room links for all four platforms', () => {
    expect(parseLiveLink('https://live.bilibili.com/22625025')).toEqual({
      platform: 'bilibili',
      roomId: '22625025',
    })
    expect(parseLiveLink('https://www.douyu.com/9999')).toEqual({
      platform: 'douyu',
      roomId: '9999',
    })
    expect(parseLiveLink('https://www.huya.com/11342412')).toEqual({
      platform: 'huya',
      roomId: '11342412',
    })
    expect(parseLiveLink('https://live.douyin.com/745963462570')).toEqual({
      platform: 'douyin',
      roomId: '745963462570',
    })
  })

  it('keeps tracking params and surrounding text', () => {
    expect(parseLiveLink('https://live.bilibili.com/123?share_source=copy_link')).toEqual({
      platform: 'bilibili',
      roomId: '123',
    })
    expect(parseLiveLink('  看这个 https://www.huya.com/660137 一个直播间  ')).toEqual({
      platform: 'huya',
      roomId: '660137',
    })
    expect(parseLiveLink('m.douyu.com/5634477')).toEqual({
      platform: 'douyu',
      roomId: '5634477',
    })
  })

  it('rejects non-room urls and plain text', () => {
    expect(parseLiveLink('')).toBeNull()
    expect(parseLiveLink('   ')).toBeNull()
    expect(parseLiveLink('一些普通文字')).toBeNull()
    expect(parseLiveLink('https://www.bilibili.com/video/BV1xx411c7mD')).toBeNull()
    // 抖音用户主页不是直播间(live. 域名限定)。
    expect(parseLiveLink('https://www.douyin.com/user/MS4wLjABAAAA')).toBeNull()
    // B 站直播首页路径 h5 不是房间号。
    expect(parseLiveLink('https://live.bilibili.com/h5')).toBeNull()
  })
})
