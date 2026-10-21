import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useLiveStore } from './live'
import { getLiveCategories, getLiveRoomDetail, getLiveRooms } from '../api/live'
import { getLiveFollows } from '../api/liveFollow'

vi.mock('../api/live', () => ({
  getLiveCategories: vi.fn(),
  getLivePlatforms: vi.fn(),
  getLiveRoomDetail: vi.fn(),
  getLiveRooms: vi.fn(),
  getLiveStreamUrls: vi.fn(),
  searchLiveRooms: vi.fn(),
}))
vi.mock('../api/liveFollow', () => ({
  followLiveRoom: vi.fn(),
  getLiveFollows: vi.fn(),
  unfollowLiveRoom: vi.fn(),
}))
vi.mock('../api/liveHistory', () => ({
  getLiveHistory: vi.fn(),
  addLiveHistory: vi.fn(),
  deleteLiveHistoryRoom: vi.fn(),
  clearLiveHistory: vi.fn(),
}))

function makeFollow(platform: string, roomId: string) {
  return { platform, roomId, roomName: `房间${roomId}`, anchorName: `主播${roomId}`, avatar: '', createdAt: 0 }
}

describe('live store follow status', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    setActivePinia(createPinia())
  })

  it('records live state per followed room and skips failed lookups', async () => {
    vi.mocked(getLiveFollows).mockResolvedValue([
      makeFollow('bilibili', '1'),
      makeFollow('huya', '2'),
      makeFollow('douyu', '3'),
    ])
    vi.mocked(getLiveRoomDetail).mockImplementation(async (_platform: string, roomId: string) => {
      if (roomId === '2') throw new Error('network down')
      return { isLive: roomId === '1' } as Awaited<ReturnType<typeof getLiveRoomDetail>>
    })

    const store = useLiveStore()
    await store.loadFollows()
    await vi.waitFor(() => {
      expect(store.followStatus['bilibili:1']).toBe(true)
    })

    expect(store.followStatus['huya:2']).toBeUndefined()
    expect(store.followStatus['douyu:3']).toBe(false)
    // 每个房间用各自的平台查询, 与当前选中平台无关。
    expect(vi.mocked(getLiveRoomDetail).mock.calls.map((call) => call[0])).toEqual([
      'bilibili',
      'huya',
      'douyu',
    ])
  })

  it('clears status when the follow list becomes empty', async () => {
    vi.mocked(getLiveFollows).mockResolvedValue([makeFollow('bilibili', '1')])
    vi.mocked(getLiveRoomDetail).mockResolvedValue({
      isLive: true,
    } as Awaited<ReturnType<typeof getLiveRoomDetail>>)

    const store = useLiveStore()
    await store.loadFollows()
    await vi.waitFor(() => expect(store.followStatus['bilibili:1']).toBe(true))

    vi.mocked(getLiveFollows).mockResolvedValue([])
    await store.loadFollows()
    expect(store.followStatus).toEqual({})
  })
})

describe('live store room pagination', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    setActivePinia(createPinia())
  })

  function rooms(count: number, platform: string) {
    return Array.from({ length: count }, (_, index) => ({
      platform,
      roomId: String(index),
    })) as unknown as Awaited<ReturnType<typeof getLiveRooms>>
  }

  it('detects more douyin pages even though its page size is 15', async () => {
    vi.mocked(getLiveRooms).mockResolvedValue(rooms(15, 'douyin'))
    const store = useLiveStore()
    store.platform = 'douyin'

    await store.loadRooms(true)
    expect(store.hasMore).toBe(true)

    // 下一页只有 10 条 = 到底了。
    vi.mocked(getLiveRooms).mockResolvedValue(rooms(10, 'douyin'))
    await store.loadMore()
    expect(store.hasMore).toBe(false)
    expect(store.rooms).toHaveLength(25)
  })

  it('still stops at bilibili page size 30', async () => {
    vi.mocked(getLiveRooms).mockResolvedValue(rooms(20, 'bilibili'))
    const store = useLiveStore()
    store.platform = 'bilibili'

    await store.loadRooms(true)
    expect(store.hasMore).toBe(false)
  })
})

describe('live store area filter', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    setActivePinia(createPinia())
    vi.mocked(getLiveRooms).mockResolvedValue([])
    vi.mocked(getLiveCategories).mockResolvedValue([
      {
        id: '1',
        name: '网游竞技',
        children: [
          { id: '101', name: '英雄联盟' },
          { id: '102', name: 'DOTA2' },
        ],
      },
    ])
  })

  it('lands on the whole group without showing a filter chip', async () => {
    const store = useLiveStore()
    for (const platform of ['bilibili', 'douyu', 'douyin']) {
      store.platform = platform
      await store.loadCategories()
      expect(store.activeAreaId).toBe('0')
      expect(store.areaFilterActive).toBe(false)
    }
  })

  it('lands on the douyin recommendation (no parent partition)', async () => {
    const store = useLiveStore()
    store.platform = 'douyin'
    await store.loadCategories()

    // 抖音没有「全部」: 空一级分区让后端落到推荐(720,1), 列表不是首个「聊天」分组。
    expect(store.activeGroupId).toBe('')
    expect(vi.mocked(getLiveRooms).mock.calls.at(-1)?.[0]).toMatchObject({
      parentAreaId: '',
      areaId: '0',
    })
  })

  it('clears a chosen sub-area back to the group', async () => {
    const store = useLiveStore()
    store.platform = 'douyu'
    await store.loadCategories()

    store.selectArea('101')
    expect(store.activeAreaId).toBe('101')
    expect(store.areaFilterActive).toBe(true)

    store.clearAreaFilter()
    expect(store.activeAreaId).toBe('0')
    expect(store.areaFilterActive).toBe(false)
    // 斗鱼清空后仍停在当前一级分区(整组列表), 不是跳回第一个分组。
    expect(store.activeGroupId).toBe('1')
  })

  it('clears a chosen douyin partition back to the recommendation', async () => {
    const store = useLiveStore()
    store.platform = 'douyin'
    await store.loadCategories()
    store.activeGroupId = '102,4'
    store.selectArea('102,4')
    expect(store.areaFilterActive).toBe(true)

    store.clearAreaFilter()
    expect(store.activeGroupId).toBe('')
    expect(store.activeAreaId).toBe('0')
    expect(store.areaFilterActive).toBe(false)
    expect(vi.mocked(getLiveRooms).mock.calls.at(-1)?.[0]).toMatchObject({
      parentAreaId: '',
      areaId: '0',
    })
  })

  it('falls back to the first game on huya, whose api needs a concrete gid', async () => {
    const store = useLiveStore()
    store.platform = 'huya'
    await store.loadCategories()
    expect(store.activeAreaId).toBe('101')
    expect(store.areaFilterActive).toBe(false)
  })
})
