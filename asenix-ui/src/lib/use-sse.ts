import { useEffect, useRef, useState, useCallback } from 'react'

export interface SseEventEntry {
  id: string
  event_type: string
  atom_id?: string
  atom_type?: string
  domain?: string
  timestamp: number
}

interface UseSSEOptions {
  types?: string[]
  ttlMs?: number
  maxRecentEvents?: number
  onEvent?: (entry: SseEventEntry) => void
}

export function useSSE({
  types = ['atom_published'],
  ttlMs = 10_000,
  maxRecentEvents = 5,
  onEvent,
}: UseSSEOptions = {}) {
  const [recentAtomIds, setRecentAtomIds] = useState<Set<string>>(new Set())
  const [recentEvents, setRecentEvents] = useState<SseEventEntry[]>([])
  const timersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map())
  const typesKey = types.join(',')

  const clearAtomId = useCallback((atomId: string) => {
    setRecentAtomIds(prev => {
      const next = new Set(prev)
      next.delete(atomId)
      return next
    })
    timersRef.current.delete(atomId)
  }, [])

  useEffect(() => {
    const qs = new URLSearchParams({ types: typesKey })
    const es = new EventSource(`/events?${qs}`)

    typesKey.split(',').forEach(eventType => {
      es.addEventListener(eventType, (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data)
          const entry: SseEventEntry = {
            id: e.lastEventId || String(Date.now()),
            event_type: eventType,
            atom_id: data.atom_id,
            atom_type: data.atom_type,
            domain: data.domain,
            timestamp: Date.now(),
          }

          if (data.atom_id) {
            setRecentAtomIds(prev => new Set(prev).add(data.atom_id))
            const existing = timersRef.current.get(data.atom_id)
            if (existing) clearTimeout(existing)
            const timer = setTimeout(() => clearAtomId(data.atom_id), ttlMs)
            timersRef.current.set(data.atom_id, timer)
          }

          setRecentEvents(prev => [entry, ...prev].slice(0, maxRecentEvents))
          onEvent?.(entry)
        } catch {
          // malformed event data — ignore
        }
      })
    })

    return () => {
      es.close()
      timersRef.current.forEach(clearTimeout)
      timersRef.current.clear()
    }
  }, [typesKey, ttlMs, maxRecentEvents, clearAtomId])

  return { recentAtomIds, recentEvents }
}
