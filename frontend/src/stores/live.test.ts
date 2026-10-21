import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useLiveStore } from './live'
import { getLiveRoomDetail } from '../api/live'
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
  getLiveHistory: vi.fn(),
  unfollowLiveRoom: vi.fn(),
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
