import { create } from 'zustand'
import type { SseEventEntry } from '#/lib/use-sse'

interface LiveFeedStore {
  recentEvents: SseEventEntry[]
  recentAtomIds: Set<string>
  setRecentEvents: (events: SseEventEntry[]) => void
  setRecentAtomIds: (ids: Set<string>) => void
}

export const useLiveFeed = create<LiveFeedStore>(set => ({
  recentEvents: [],
  recentAtomIds: new Set(),
  setRecentEvents: events => set({ recentEvents: events }),
  setRecentAtomIds: ids => set({ recentAtomIds: ids }),
}))
