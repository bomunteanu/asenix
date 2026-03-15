import { create } from 'zustand'
import type { SseEventEntry } from '#/lib/use-sse'

interface LiveFeedStore {
  recentEvents: SseEventEntry[]
  setRecentEvents: (events: SseEventEntry[]) => void
}

export const useLiveFeed = create<LiveFeedStore>(set => ({
  recentEvents: [],
  setRecentEvents: events => set({ recentEvents: events }),
}))
