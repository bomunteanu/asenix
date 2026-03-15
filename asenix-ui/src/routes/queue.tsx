import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { ChevronDown, ChevronRight, Check, X, AlertTriangle, HelpCircle } from 'lucide-react'

export const Route = createFileRoute('/queue')({
  component: QueueComponent,
})

function HelpModal({ onClose }: { onClose: () => void }) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="bg-[var(--bg)] border border-[var(--border)] rounded-xl shadow-lg max-w-md w-full p-6 space-y-4" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-medium text-[var(--text-primary)]">Review Queue</h2>
          <button onClick={onClose} className="text-[var(--text-muted)] hover:text-[var(--text-primary)]"><X className="w-4 h-4" /></button>
        </div>
        <div className="space-y-3 text-sm text-[var(--text-muted)]">
          <p>
            <span className="text-[var(--text-primary)] font-medium">Pending atoms</span> — newly published atoms awaiting human review. Approving or rejecting persists a review record and updates the author's reliability score.
          </p>
          <p>
            <span className="text-[var(--text-primary)] font-medium">✓ Approve</span> — marks the atom as approved and slightly increases the author's reliability score.
          </p>
          <p>
            <span className="text-[var(--text-primary)] font-medium">✗ Reject</span> — marks the atom as rejected and decreases the author's reliability score. The atom stays in the DB but is flagged.
          </p>
          <p>
            <span className="text-[var(--text-primary)] font-medium">Contradictions</span> — atoms in the <em>contested</em> lifecycle. These were automatically flagged by the server when conflicting findings were detected under equivalent conditions.
          </p>
        </div>
        <button onClick={onClose} className="w-full py-2 border border-[var(--border)] text-[var(--text-primary)] rounded-lg text-sm hover:bg-[var(--bg-subtle)] transition-colors">Got it</button>
      </div>
    </div>
  )
}

function QueueComponent() {
  const [pendingExpanded, setPendingExpanded] = useState(true)
  const [contestedExpanded, setContestedExpanded] = useState(true)
  const [showHelp, setShowHelp] = useState(false)
  const [atomOutcomes, setAtomOutcomes] = useState<Record<string, 'approved' | 'rejected'>>({})

  const queryClient = useQueryClient()

  // Use the dedicated /review endpoint — returns atoms with review_status='pending'
  const { data: reviewData, isLoading: reviewLoading } = useQuery({
    queryKey: ['reviewQueue'],
    queryFn: () => jsonRpcClient.getReviewQueue({ limit: 50 }),
    refetchInterval: 30000,
  })

  const { data: contestedData, isLoading: contestedLoading } = useQuery({
    queryKey: ['contestedAtoms'],
    queryFn: () => jsonRpcClient.searchAtoms({ lifecycle: 'contested', limit: 50 }),
    refetchInterval: 30000,
  })

  const approveMutation = useMutation({
    mutationFn: (atomId: string) => jsonRpcClient.reviewAtom(atomId, 'approve'),
    onSuccess: (_data, atomId) => {
      setAtomOutcomes(prev => ({ ...prev, [atomId]: 'approved' }))
      queryClient.invalidateQueries({ queryKey: ['reviewQueue'] })
    },
  })

  const rejectMutation = useMutation({
    mutationFn: (atomId: string) => jsonRpcClient.reviewAtom(atomId, 'reject'),
    onSuccess: (_data, atomId) => {
      setAtomOutcomes(prev => ({ ...prev, [atomId]: 'rejected' }))
      queryClient.invalidateQueries({ queryKey: ['reviewQueue'] })
    },
  })

  const banMutation = useMutation({
    mutationFn: (atomId: string) => jsonRpcClient.banAtom(atomId),
    onSuccess: (_data, atomId) => {
      setAtomOutcomes(prev => ({ ...prev, [atomId]: 'rejected' }))
      queryClient.invalidateQueries({ queryKey: ['contestedAtoms'] })
    },
  })

  const formatRelativeTime = (ts?: string) => {
    if (!ts) return ''
    const diffMs = Date.now() - new Date(ts).getTime()
    const h = Math.floor(diffMs / 3600000)
    const d = Math.floor(h / 24)
    if (d > 0) return `${d}d ago`
    if (h > 0) return `${h}h ago`
    return 'just now'
  }

  const pendingItems: any[] = reviewData?.items ?? []
  const contestedAtoms = contestedData?.atoms ?? []
  const allEmpty = pendingItems.length === 0 && contestedAtoms.length === 0

  return (
    <div className="p-6">
      {showHelp && <HelpModal onClose={() => setShowHelp(false)} />}

      <div className="mb-6 flex items-start justify-between">
        <div>
          <h1 className="text-2xl font-light tracking-tight text-[var(--text-primary)] mb-1">Review Queue</h1>
          <p className="text-[var(--text-muted)]">Review and moderate incoming research contributions</p>
        </div>
        <button onClick={() => setShowHelp(true)} className="p-2 text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors" title="Help">
          <HelpCircle className="w-5 h-5" />
        </button>
      </div>

      {allEmpty && !reviewLoading && !contestedLoading ? (
        <Card>
          <CardContent className="text-center py-12">
            <div className="w-12 h-12 bg-[var(--accent)] rounded-full flex items-center justify-center mx-auto mb-4">
              <Check className="w-6 h-6 text-white" />
            </div>
            <h3 className="text-lg font-medium text-[var(--text-primary)] mb-1">Queue is clear</h3>
            <p className="text-[var(--text-muted)] text-sm">No items pending review</p>
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-4">

          {/* Pending review */}
          <Card>
            <CardHeader className="cursor-pointer hover:bg-[var(--bg-subtle)] transition-colors" onClick={() => setPendingExpanded(v => !v)}>
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <AlertTriangle className="w-4 h-4 text-[var(--text-muted)]" />
                  <CardTitle>Pending Review</CardTitle>
                  {pendingItems.length > 0 && (
                    <span className="bg-[var(--accent)] text-white text-xs px-2 py-0.5 rounded-full">{pendingItems.length}</span>
                  )}
                </div>
                {pendingExpanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
              </div>
            </CardHeader>
            {pendingExpanded && (
              <CardContent>
                {reviewLoading ? (
                  <div className="text-center py-8 text-[var(--text-muted)]">Loading…</div>
                ) : pendingItems.length === 0 ? (
                  <div className="text-center py-8 text-[var(--text-muted)] text-sm">No pending atoms</div>
                ) : (
                  <div className="space-y-3">
                    {pendingItems.map((item: any) => {
                      const outcome = atomOutcomes[item.atom_id]
                      return (
                        <div key={item.atom_id} className={`border rounded-lg p-4 transition-all ${
                          outcome === 'approved' ? 'border-[var(--accent)] bg-[var(--bg-subtle)]'
                          : outcome === 'rejected' ? 'border-[var(--danger)] opacity-40'
                          : 'border-[var(--border)] bg-[var(--bg)]'
                        }`}>
                          <div className="flex justify-between items-start gap-4">
                            <div className="flex-1 min-w-0">
                              <div className="flex items-center gap-2 mb-1 flex-wrap">
                                <span className="text-xs font-mono text-[var(--text-muted)]">{item.atom_id?.slice(0, 10)}…</span>
                                <span className="text-xs text-[var(--text-muted)]">{item.atom_type}</span>
                                <span className="text-xs font-mono text-[var(--accent)]">{item.domain}</span>
                                <span className="text-xs text-[var(--text-muted)]">{formatRelativeTime(item.created_at)}</span>
                                {item.auto_review_eligible && (
                                  <span className="text-xs border border-[var(--accent)] text-[var(--accent)] rounded px-1.5 py-0.5">trusted author</span>
                                )}
                              </div>
                              <p className="text-sm text-[var(--text-primary)] leading-relaxed line-clamp-3">{item.statement}</p>
                            </div>
                            <div className="flex flex-col gap-2 flex-shrink-0">
                              {outcome ? (
                                <span className={`text-xs px-2 py-1 rounded font-medium ${outcome === 'approved' ? 'text-[var(--accent)]' : 'text-[var(--danger)]'}`}>
                                  {outcome === 'approved' ? 'Approved' : 'Rejected'}
                                </span>
                              ) : (
                                <>
                                  <button
                                    onClick={() => approveMutation.mutate(item.atom_id)}
                                    disabled={approveMutation.isPending}
                                    className="p-2 text-[var(--accent)] border border-[var(--accent)] rounded-lg hover:bg-[var(--accent)] hover:text-white transition-colors disabled:opacity-40"
                                    title="Approve — persists review, updates author reliability"
                                  >
                                    <Check className="w-4 h-4" />
                                  </button>
                                  <button
                                    onClick={() => rejectMutation.mutate(item.atom_id)}
                                    disabled={rejectMutation.isPending}
                                    className="p-2 text-[var(--danger)] border border-[var(--danger)] rounded-lg hover:bg-[var(--danger)] hover:text-white transition-colors disabled:opacity-40"
                                    title="Reject — persists review, decreases author reliability"
                                  >
                                    <X className="w-4 h-4" />
                                  </button>
                                </>
                              )}
                            </div>
                          </div>
                        </div>
                      )
                    })}
                  </div>
                )}
              </CardContent>
            )}
          </Card>

          {/* Contradictions */}
          <Card>
            <CardHeader className="cursor-pointer hover:bg-[var(--bg-subtle)] transition-colors" onClick={() => setContestedExpanded(v => !v)}>
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <AlertTriangle className="w-4 h-4 text-[var(--danger)]" />
                  <CardTitle>Contradictions</CardTitle>
                  {contestedAtoms.length > 0 && (
                    <span className="bg-[var(--danger)] text-white text-xs px-2 py-0.5 rounded-full">{contestedAtoms.length}</span>
                  )}
                </div>
                {contestedExpanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
              </div>
            </CardHeader>
            {contestedExpanded && (
              <CardContent>
                {contestedLoading ? (
                  <div className="text-center py-8 text-[var(--text-muted)]">Loading…</div>
                ) : contestedAtoms.length === 0 ? (
                  <div className="text-center py-8 text-[var(--text-muted)] text-sm">No contradictions</div>
                ) : (
                  <div className="space-y-3">
                    {contestedAtoms.map(atom => (
                      <div key={atom.atom_id} className="border border-[var(--danger)] rounded-lg p-4 bg-[var(--bg)]">
                        <div className="flex justify-between items-start gap-4">
                          <div className="flex-1">
                            <div className="flex items-center gap-2 mb-1 flex-wrap">
                              <span className="text-xs font-mono text-[var(--text-muted)]">{atom.atom_id.slice(0, 10)}…</span>
                              <span className="text-xs font-mono text-[var(--accent)]">{atom.domain}</span>
                              <span className="text-xs text-[var(--text-muted)]">{atom.atom_type}</span>
                            </div>
                            <p className="text-sm text-[var(--text-primary)] leading-relaxed line-clamp-3">{atom.statement}</p>
                          </div>
                          <button
                            onClick={() => banMutation.mutate(atom.atom_id)}
                            disabled={banMutation.isPending || !!atomOutcomes[atom.atom_id]}
                            className="p-2 text-[var(--danger)] border border-[var(--danger)] rounded-lg hover:bg-[var(--danger)] hover:text-white transition-colors disabled:opacity-40 flex-shrink-0"
                            title="Ban contested atom"
                          >
                            {atomOutcomes[atom.atom_id] ? <Check className="w-4 h-4" /> : <X className="w-4 h-4" />}
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </CardContent>
            )}
          </Card>

        </div>
      )}
    </div>
  )
}
