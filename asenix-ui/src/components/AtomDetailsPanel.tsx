import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { useState, useEffect } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import type { Atom } from '#/lib/bindings'
import { getChartColors } from '#/lib/chart-utils'
import { jsonRpcClient } from '#/lib/json-rpc-client'

interface AtomDetailsPanelProps {
  atom: Atom | null
  onClose: () => void
  onNodeSelect?: (atomId: string) => void
}

export default function AtomDetailsPanel({ atom, onClose, onNodeSelect }: AtomDetailsPanelProps) {
  const [banConfirmation, setBanConfirmation] = useState('')
  const [showBanConfirm, setShowBanConfirm] = useState(false)
  const [showRemoveConfirm, setShowRemoveConfirm] = useState(false)
  const queryClient = useQueryClient()

  // Close on Escape key
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose()
      }
    }
    document.addEventListener('keydown', handleEscape)
    return () => document.removeEventListener('keydown', handleEscape)
  }, [onClose])

  // Close on outside click
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      const target = e.target as HTMLElement
      if (!target.closest('.atom-inspector-panel')) {
        onClose()
      }
    }
    if (atom) {
      document.addEventListener('mousedown', handleClickOutside)
    }
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [atom, onClose])

  const banMutation = useMutation({
    mutationFn: (atomId: string) => jsonRpcClient.banAtom(atomId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['graph'] })
      queryClient.invalidateQueries({ queryKey: ['bounties'] })
      queryClient.invalidateQueries({ queryKey: ['pendingAtoms'] })
      onClose()
    },
  })

  const unbanMutation = useMutation({
    mutationFn: (atomId: string) => jsonRpcClient.unbanAtom(atomId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['graph'] })
      queryClient.invalidateQueries({ queryKey: ['bounties'] })
    },
  })

  const removeMutation = useMutation({
    mutationFn: (atomId: string) => jsonRpcClient.banAtom(atomId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['graph'] })
      queryClient.invalidateQueries({ queryKey: ['bounties'] })
      queryClient.invalidateQueries({ queryKey: ['pendingAtoms'] })
      onClose()
    },
  })

  if (!atom) return null

  const chartColors = getChartColors()
  const getNodeColor = (atomType: string) => {
    switch (atomType) {
      case 'bounty': return chartColors.nodeBounty
      case 'finding': return chartColors.nodeFinding
      case 'hypothesis': return chartColors.nodeHypothesis
      case 'negative_result': return chartColors.nodeNegativeResult
      case 'synthesis': return chartColors.nodeSynthesis
      default: return chartColors.textMuted
    }
  }

  const getLifecycleColor = (lifecycle: string) => {
    switch (lifecycle) {
      case 'provisional': return 'border-[var(--text-muted)] text-[var(--text-muted)]'
      case 'replicated': return 'border-[var(--accent)] text-[var(--accent)]'
      case 'core': return 'border-[var(--accent)] bg-[var(--accent)] text-white'
      case 'contested': return 'border-[var(--danger)] text-[var(--danger)]'
      default: return 'border-[var(--text-muted)] text-[var(--text-muted)]'
    }
  }

  const formatMetricValue = (metric: any) => {
    if (typeof metric.value === 'number') {
      if (metric.name?.includes('accuracy')) {
        return `${(metric.value * 100).toFixed(2)}%`
      } else if (metric.name?.includes('loss')) {
        return metric.value.toFixed(6)
      } else if (metric.name?.includes('time')) {
        return `${metric.value.toFixed(0)}s`
      } else if (metric.name?.includes('params')) {
        return metric.value.toLocaleString()
      }
    }
    return String(metric.value)
  }

  const getMetricDirection = (metric: any) => {
    // For accuracy, higher is better; for loss, lower is better
    if (metric.name?.includes('accuracy')) return 'up'
    if (metric.name?.includes('loss')) return 'down'
    return null
  }

  const handleParentClick = (parentId: string) => {
    if (onNodeSelect) {
      onNodeSelect(parentId)
    }
  }

  const handleBan = () => {
    if (banConfirmation !== 'BAN') return
    banMutation.mutate(atom!.atom_id)
    setShowBanConfirm(false)
    setBanConfirmation('')
  }

  const metrics = atom.metrics || []
  const conditions = atom.conditions || {}
  const pheromoneValues = {
    attraction: atom.ph_attraction ?? 0,
    repulsion: atom.ph_repulsion ?? 0,
    novelty: atom.ph_novelty ?? 0,
    disagreement: atom.ph_disagreement ?? 0,
  }

  // Extract provenance data
  const provenance = conditions.provenance || {}
  const parentIds = provenance.parent_ids || []
  const methodDescription = provenance.method_description || 'Unknown method'
  const hardware = provenance.environment?.hardware || 'Unknown hardware'

  return (
    <div className="fixed inset-y-0 right-0 w-[400px] bg-[var(--bg)] border-l border-[var(--border)] shadow-sm z-50 overflow-y-auto atom-inspector-panel">
      <div className="relative p-6">
        {/* Close button */}
        <button
          onClick={onClose}
          className="absolute top-4 right-4 text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors"
        >
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
            <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
          </svg>
        </button>

        {/* HEADER */}
        <div className="mb-6">
          <div className="flex items-center gap-2 mb-3">
            {/* Atom type badge */}
            <span 
              className="px-2 py-1 rounded text-xs font-medium text-white"
              style={{ backgroundColor: getNodeColor(atom.atom_type) }}
            >
              {atom.atom_type}
            </span>
            
            {/* Lifecycle badge */}
            <span className={`px-2 py-1 rounded text-xs font-medium border ${getLifecycleColor(atom.lifecycle)}`}>
              {atom.lifecycle}
            </span>
          </div>
          
          {/* Atom ID */}
          <div className="text-xs font-mono text-[var(--text-muted)]">
            {atom.atom_id.slice(0, 16)}...
          </div>
        </div>

        <div className="space-y-6">
          {/* STATEMENT */}
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">Statement</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-[var(--text-primary)] leading-relaxed">
                {atom.statement}
              </p>
            </CardContent>
          </Card>

          {/* CONDITIONS */}
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">Conditions</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-1">
                {Object.entries(conditions)
                  .filter(([key]) => !key.startsWith('ph_') && key !== 'provenance')
                  .sort(([a], [b]) => {
                    // Required keys first (optimizer, scheduler, learning_rate, etc.)
                    const requiredKeys = ['optimizer', 'scheduler', 'learning_rate', 'num_blocks', 'base_channels', 'augmentation']
                    const aRequired = requiredKeys.includes(a)
                    const bRequired = requiredKeys.includes(b)
                    if (aRequired && !bRequired) return -1
                    if (!aRequired && bRequired) return 1
                    return a.localeCompare(b)
                  })
                  .map(([key, value]) => (
                    <div key={key} className="flex justify-between text-sm">
                      <span className="text-[var(--text-muted)]">{key}</span>
                      <span className={`font-medium ${!isNaN(Number(value)) ? 'text-right' : ''}`}>
                        {String(value)}
                      </span>
                    </div>
                  ))}
              </div>
            </CardContent>
          </Card>

          {/* METRICS */}
          {metrics.length > 0 && (
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium">Metrics</CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                {metrics.map((metric: any, index: number) => {
                  const direction = getMetricDirection(metric)
                  return (
                    <div key={index} className="flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <span className="text-sm text-[var(--text-muted)]">{metric.name}</span>
                        {direction && (
                          <span className={`text-xs ${direction === 'up' ? 'text-green-500' : 'text-red-500'}`}>
                            {direction === 'up' ? '↑' : '↓'}
                          </span>
                        )}
                      </div>
                      <span className="text-sm font-medium">
                        {formatMetricValue(metric)}
                      </span>
                    </div>
                  )
                })}
              </CardContent>
            </Card>
          )}

          {/* PHEROMONE */}
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">Pheromone</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 gap-3">
                {Object.entries(pheromoneValues).map(([key, value]) => (
                  <div key={key} className="space-y-1">
                    <div className="flex justify-between items-center">
                      <span className="text-xs text-[var(--text-muted)] capitalize">
                        {key.replace('_', ' ')}
                      </span>
                      <span className="text-xs font-medium">
                        {Number(value).toFixed(3)}
                      </span>
                    </div>
                    <div className="w-full bg-[var(--border)] rounded-full h-1">
                      <div 
                        className="bg-[var(--accent)] h-1 rounded-full transition-all duration-300"
                        style={{ width: `${Math.min(Number(value), 1) * 100}%` }}
                      />
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>

          {/* PROVENANCE */}
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">Provenance</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              {parentIds.length > 0 && (
                <div>
                  <div className="text-xs text-[var(--text-muted)] mb-2">Parent IDs</div>
                  <div className="flex flex-wrap gap-1">
                    {parentIds.map((parentId: string, index: number) => (
                      <button
                        key={index}
                        onClick={() => handleParentClick(parentId)}
                        className="px-2 py-1 bg-[var(--bg-subtle)] border border-[var(--border)] rounded text-xs font-mono hover:bg-[var(--accent)] hover:text-white transition-colors cursor-pointer"
                      >
                        {parentId.slice(0, 8)}...
                      </button>
                    ))}
                  </div>
                </div>
              )}
              
              <div>
                <div className="text-xs text-[var(--text-muted)] mb-1">Method</div>
                <div className="text-sm text-[var(--text-primary)]">
                  {methodDescription}
                </div>
              </div>
              
              <div>
                <div className="text-xs text-[var(--text-muted)] mb-1">Hardware</div>
                <div className="text-sm text-[var(--text-primary)]">
                  {hardware}
                </div>
              </div>
            </CardContent>
          </Card>

          {/* ACTIONS */}
          <div className="pt-4 border-t border-[var(--border)] space-y-2">
            {/* Remove Atom */}
            {!showRemoveConfirm ? (
              <button
                onClick={() => setShowRemoveConfirm(true)}
                className="w-full py-2 border border-[var(--border)] text-[var(--text-muted)] rounded-lg font-medium hover:border-[var(--danger)] hover:text-[var(--danger)] transition-colors"
              >
                Remove Atom
              </button>
            ) : (
              <div className="flex gap-2">
                <button
                  onClick={() => { removeMutation.mutate(atom.atom_id); setShowRemoveConfirm(false) }}
                  disabled={removeMutation.isPending}
                  className="flex-1 py-2 bg-[var(--danger)] text-white rounded-lg font-medium disabled:opacity-50 hover:bg-opacity-90 transition-colors"
                >
                  {removeMutation.isPending ? 'Removing...' : 'Confirm Remove'}
                </button>
                <button
                  onClick={() => setShowRemoveConfirm(false)}
                  className="flex-1 py-2 border border-[var(--border)] text-[var(--text-primary)] rounded-lg font-medium hover:bg-[var(--bg-subtle)] transition-colors"
                >
                  Cancel
                </button>
              </div>
            )}

            {/* Ban / Unban */}
            {atom.ban_flag ? (
              <button
                onClick={() => unbanMutation.mutate(atom.atom_id)}
                disabled={unbanMutation.isPending}
                className="w-full py-2 border border-[var(--accent)] text-[var(--accent)] rounded-lg font-medium hover:bg-[var(--accent)] hover:text-white transition-colors disabled:opacity-50"
              >
                {unbanMutation.isPending ? 'Unbanning...' : 'Unban Atom'}
              </button>
            ) : !showBanConfirm ? (
              <button
                onClick={() => setShowBanConfirm(true)}
                className="w-full py-2 border border-[var(--danger)] text-[var(--danger)] rounded-lg font-medium hover:bg-[var(--danger)] hover:text-white transition-colors"
              >
                Ban Atom
              </button>
            ) : (
              <div className="space-y-3">
                <div className="text-sm text-[var(--text-muted)]">
                  Type <span className="font-mono bg-[var(--bg-subtle)] px-1">BAN</span> to confirm:
                </div>
                <input
                  type="text"
                  value={banConfirmation}
                  onChange={(e) => setBanConfirmation(e.target.value)}
                  placeholder="Type BAN"
                  className="w-full p-2 border border-[var(--border)] rounded bg-[var(--bg)] text-[var(--text-primary)] placeholder-[var(--text-muted)]"
                  autoFocus
                />
                <div className="flex gap-2">
                  <button
                    onClick={handleBan}
                    disabled={banConfirmation !== 'BAN' || banMutation.isPending}
                    className="flex-1 py-2 bg-[var(--danger)] text-white rounded-lg font-medium disabled:opacity-50 disabled:cursor-not-allowed hover:bg-opacity-90 transition-colors"
                  >
                    {banMutation.isPending ? 'Banning...' : 'Confirm Ban'}
                  </button>
                  <button
                    onClick={() => { setShowBanConfirm(false); setBanConfirmation('') }}
                    className="flex-1 py-2 border border-[var(--border)] text-[var(--text-primary)] rounded-lg font-medium hover:bg-[var(--bg-subtle)] transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
