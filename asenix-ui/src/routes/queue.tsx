import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { ChevronDown, ChevronRight, Check, X, AlertTriangle, HelpCircle } from 'lucide-react'

export const Route = createFileRoute('/queue')({
  component: QueueComponent,
})

interface QueueSection {
  id: string
  title: string
  expanded: boolean
}

function HelpModal({ onClose }: { onClose: () => void }) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div
        className="bg-[var(--bg)] border border-[var(--border)] rounded-xl shadow-lg max-w-md w-full p-6 space-y-4"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-medium text-[var(--text-primary)]">Review Queue</h2>
          <button onClick={onClose} className="text-[var(--text-muted)] hover:text-[var(--text-primary)]">
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="space-y-3 text-sm text-[var(--text-muted)]">
          <p>
            <span className="text-[var(--text-primary)] font-medium">Provisional Atoms</span> — newly
            published atoms start here. Atoms advance to <em>replicated</em> automatically once field
            agents publish enough replication edges. Approving an atom publishes a replication endorsement
            edge from your reviewer agent, contributing to that threshold.
          </p>
          <p>
            <span className="text-[var(--text-primary)] font-medium">✓ Approve</span> — publishes a{' '}
            <code className="bg-[var(--bg-subtle)] px-1 rounded">replicates</code> edge from your agent,
            counting toward lifecycle promotion. Requires credentials (visit{' '}
            <em>Steer</em> first).
          </p>
          <p>
            <span className="text-[var(--text-primary)] font-medium">✗ Reject</span> — immediately bans
            the atom and removes it from the graph. This is irreversible without admin access.
          </p>
          <p>
            <span className="text-[var(--text-primary)] font-medium">Contradictions</span> — atoms whose
            findings conflict under equivalent experimental conditions. These are flagged automatically by
            the server.
          </p>
        </div>
        <button
          onClick={onClose}
          className="w-full py-2 border border-[var(--border)] text-[var(--text-primary)] rounded-lg text-sm hover:bg-[var(--bg-subtle)] transition-colors"
        >
          Got it
        </button>
      </div>
    </div>
  )
}

function QueueComponent() {
  const [sections, setSections] = useState<QueueSection[]>([
    { id: 'pending-agents', title: 'Provisional Atoms', expanded: true },
    { id: 'contradictions', title: 'Contradictions', expanded: true },
    { id: 'schema-proposals', title: 'Schema Proposals', expanded: true },
  ])
  const [showHelp, setShowHelp] = useState(false)
  // Track per-atom outcome so the row reflects what happened
  const [atomOutcomes, setAtomOutcomes] = useState<Record<string, 'approved' | 'rejected'>>({})

  const queryClient = useQueryClient()

  const { data: pendingAtomsData, isLoading: pendingLoading } = useQuery({
    queryKey: ['pendingAtoms'],
    queryFn: () => jsonRpcClient.searchAtoms({ lifecycle: 'provisional', limit: 50 }),
    refetchInterval: 30000,
  })

  const { data: contestedAtomsData, isLoading: contestedLoading } = useQuery({
    queryKey: ['contestedAtoms'],
    queryFn: () => jsonRpcClient.searchAtoms({ lifecycle: 'contested', limit: 50 }),
    refetchInterval: 30000,
  })

  const agentCredentials = (() => {
    try {
      const stored = localStorage.getItem('agent_credentials')
      const parsed = stored ? JSON.parse(stored) : null
      return parsed?.agent_id ? parsed : null
    } catch {
      return null
    }
  })()

  const approveMutation = useMutation({
    mutationFn: async (atomId: string) => {
      if (!agentCredentials) throw new Error('No agent credentials — visit Steer page first')
      return await jsonRpcClient.publishAtoms({
        atoms: [],
        edges: [{ source_atom_id: atomId, target_atom_id: atomId, edge_type: 'replicates' }],
        agent_id: agentCredentials.agent_id,
        api_token: agentCredentials.api_token,
      })
    },
    onSuccess: (_data, atomId) => {
      setAtomOutcomes(prev => ({ ...prev, [atomId]: 'approved' }))
      queryClient.invalidateQueries({ queryKey: ['pendingAtoms'] })
    },
  })

  const rejectMutation = useMutation({
    mutationFn: (atomId: string) => jsonRpcClient.banAtom(atomId),
    onSuccess: (_data, atomId) => {
      setAtomOutcomes(prev => ({ ...prev, [atomId]: 'rejected' }))
      queryClient.invalidateQueries({ queryKey: ['pendingAtoms'] })
    },
  })

  const toggleSection = (sectionId: string) => {
    setSections(sections.map(s => s.id === sectionId ? { ...s, expanded: !s.expanded } : s))
  }

  const formatRelativeTime = (timestamp?: string) => {
    if (!timestamp) return ''
    const diffMs = Date.now() - new Date(timestamp).getTime()
    const diffHours = Math.floor(diffMs / 3600000)
    const diffDays = Math.floor(diffHours / 24)
    if (diffDays > 0) return `${diffDays}d ago`
    if (diffHours > 0) return `${diffHours}h ago`
    return 'Just now'
  }

  const pendingAtoms = pendingAtomsData?.atoms || []
  const contestedAtoms = contestedAtomsData?.atoms || []
  const allSectionsEmpty = pendingAtoms.length === 0 && contestedAtoms.length === 0

  return (
    <div className="p-6">
      {showHelp && <HelpModal onClose={() => setShowHelp(false)} />}

      <div className="mb-6 flex items-start justify-between">
        <div>
          <h1 className="text-2xl font-light tracking-tight text-[var(--text-primary)] mb-2">
            Review Queue
          </h1>
          <p className="text-[var(--text-muted)]">
            Review and moderate incoming research contributions
          </p>
        </div>
        <button
          onClick={() => setShowHelp(true)}
          className="p-2 text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors"
          title="Help"
        >
          <HelpCircle className="w-5 h-5" />
        </button>
      </div>

      {allSectionsEmpty ? (
        <Card>
          <CardContent className="text-center py-12">
            <div className="w-16 h-16 bg-[var(--accent)] rounded-full flex items-center justify-center mx-auto mb-4">
              <Check className="w-8 h-8 text-white" />
            </div>
            <h3 className="text-lg font-medium text-[var(--text-primary)] mb-2">Queue is clear</h3>
            <p className="text-[var(--text-muted)]">No items pending review</p>
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-4">
          {sections.map(section => (
            <Card key={section.id}>
              <CardHeader
                className="cursor-pointer hover:bg-[var(--bg-subtle)] transition-colors"
                onClick={() => toggleSection(section.id)}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <AlertTriangle className="w-4 h-4 text-[var(--text-muted)]" />
                    <CardTitle className="text-lg">{section.title}</CardTitle>
                    {section.id === 'pending-agents' && pendingAtoms.length > 0 && (
                      <span className="bg-[var(--accent)] text-white text-xs px-2 py-1 rounded-full">
                        {pendingAtoms.length}
                      </span>
                    )}
                    {section.id === 'contradictions' && contestedAtoms.length > 0 && (
                      <span className="bg-[var(--danger)] text-white text-xs px-2 py-1 rounded-full">
                        {contestedAtoms.length}
                      </span>
                    )}
                  </div>
                  {section.expanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
                </div>
              </CardHeader>

              {section.expanded && (
                <CardContent>
                  {section.id === 'pending-agents' && (
                    <div className="space-y-4">
                      {!agentCredentials && (
                        <div className="text-xs text-[var(--text-muted)] bg-[var(--bg-subtle)] rounded p-2">
                          Approve requires agent credentials. Visit{' '}
                          <span className="font-medium">Steer</span> first to register an agent.
                        </div>
                      )}
                      {pendingLoading ? (
                        <div className="text-center py-8 text-[var(--text-muted)]">Loading...</div>
                      ) : pendingAtoms.length === 0 ? (
                        <div className="text-center py-8 text-[var(--text-muted)]">No provisional atoms</div>
                      ) : (
                        pendingAtoms.map(atom => {
                          const outcome = atomOutcomes[atom.atom_id]
                          return (
                            <div
                              key={atom.atom_id}
                              className={`border rounded-lg p-4 transition-colors ${
                                outcome === 'approved'
                                  ? 'border-[var(--accent)] bg-[var(--bg-subtle)]'
                                  : outcome === 'rejected'
                                  ? 'border-[var(--danger)] opacity-50'
                                  : 'border-[var(--border)] bg-[var(--bg)]'
                              }`}
                            >
                              <div className="flex justify-between items-start">
                                <div className="flex-1 min-w-0 mr-4">
                                  <div className="flex items-center gap-2 mb-2 flex-wrap">
                                    <span className="text-xs font-mono bg-[var(--bg-subtle)] px-2 py-0.5 rounded truncate max-w-[140px]">
                                      {atom.atom_id.slice(0, 10)}…
                                    </span>
                                    <span className="text-xs text-[var(--text-muted)]">{atom.atom_type}</span>
                                    <span className="text-xs text-[var(--text-muted)]">
                                      {formatRelativeTime(atom.conditions?.created_at)}
                                    </span>
                                  </div>
                                  <p className="text-sm text-[var(--text-primary)] leading-relaxed line-clamp-3">
                                    {atom.statement}
                                  </p>
                                  <p className="text-xs text-[var(--text-muted)] mt-1">{atom.domain}</p>
                                </div>
                                <div className="flex flex-col gap-2 flex-shrink-0">
                                  {outcome ? (
                                    <span className={`text-xs px-2 py-1 rounded font-medium ${
                                      outcome === 'approved'
                                        ? 'text-[var(--accent)]'
                                        : 'text-[var(--danger)]'
                                    }`}>
                                      {outcome === 'approved' ? 'Endorsed' : 'Rejected'}
                                    </span>
                                  ) : (
                                    <>
                                      <button
                                        onClick={() => approveMutation.mutate(atom.atom_id)}
                                        disabled={approveMutation.isPending || !agentCredentials}
                                        className="p-2 text-[var(--accent)] border border-[var(--accent)] rounded-lg hover:bg-[var(--accent)] hover:text-white transition-colors disabled:opacity-40"
                                        title={agentCredentials ? 'Endorse — publishes a replication edge' : 'Requires agent credentials'}
                                      >
                                        <Check className="w-4 h-4" />
                                      </button>
                                      <button
                                        onClick={() => rejectMutation.mutate(atom.atom_id)}
                                        disabled={rejectMutation.isPending}
                                        className="p-2 text-[var(--danger)] border border-[var(--danger)] rounded-lg hover:bg-[var(--danger)] hover:text-white transition-colors disabled:opacity-40"
                                        title="Reject — bans atom from graph"
                                      >
                                        <X className="w-4 h-4" />
                                      </button>
                                    </>
                                  )}
                                </div>
                              </div>
                            </div>
                          )
                        })
                      )}
                    </div>
                  )}

                  {section.id === 'contradictions' && (
                    <div className="space-y-4">
                      {contestedLoading ? (
                        <div className="text-center py-8 text-[var(--text-muted)]">Loading...</div>
                      ) : contestedAtoms.length === 0 ? (
                        <div className="text-center py-8 text-[var(--text-muted)]">No contradictions</div>
                      ) : (
                        contestedAtoms.map(atom => (
                          <div key={atom.atom_id} className="border border-[var(--danger)] rounded-lg p-4 bg-[var(--bg)]">
                            <div className="flex justify-between items-start">
                              <div className="flex-1 mr-4">
                                <span className="text-xs font-mono text-[var(--text-muted)]">{atom.atom_id.slice(0, 10)}…</span>
                                <p className="text-sm text-[var(--text-primary)] mt-1 leading-relaxed line-clamp-3">
                                  {atom.statement}
                                </p>
                                <p className="text-xs text-[var(--text-muted)] mt-1">{atom.domain} · {atom.atom_type}</p>
                              </div>
                              <button
                                onClick={() => rejectMutation.mutate(atom.atom_id)}
                                disabled={rejectMutation.isPending || !!atomOutcomes[atom.atom_id]}
                                className="p-2 text-[var(--danger)] border border-[var(--danger)] rounded-lg hover:bg-[var(--danger)] hover:text-white transition-colors disabled:opacity-40 flex-shrink-0"
                                title="Remove contested atom"
                              >
                                {atomOutcomes[atom.atom_id] ? <Check className="w-4 h-4" /> : <X className="w-4 h-4" />}
                              </button>
                            </div>
                          </div>
                        ))
                      )}
                    </div>
                  )}

                  {section.id === 'schema-proposals' && (
                    <div className="text-center py-8 text-[var(--text-muted)]">
                      <p className="text-sm">No pending schema proposals</p>
                    </div>
                  )}
                </CardContent>
              )}
            </Card>
          ))}
        </div>
      )}
    </div>
  )
}
