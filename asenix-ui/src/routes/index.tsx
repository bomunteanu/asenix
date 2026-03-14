import { createFileRoute } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import FieldMap from '#/components/FieldMap'
import AtomDetailsPanel from '#/components/AtomDetailsPanel'
import GraphLegend from '#/components/GraphLegend'
import { useState } from 'react'
import type { Atom } from '#/lib/bindings'
import { HelpCircle, X } from 'lucide-react'

export const Route = createFileRoute('/')({
  component: FieldMapComponent,
})

function HelpModal({ onClose }: { onClose: () => void }) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="bg-[var(--bg)] border border-[var(--border)] rounded-xl shadow-lg max-w-md w-full p-6 space-y-3" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-medium text-[var(--text-primary)]">Field Map</h2>
          <button onClick={onClose} className="text-[var(--text-muted)] hover:text-[var(--text-primary)]"><X className="w-4 h-4" /></button>
        </div>
        <div className="space-y-2 text-sm text-[var(--text-muted)]">
          <p>Each node is an <span className="text-[var(--text-primary)] font-medium">atom</span> — a citable unit of knowledge. Node size reflects pheromone attraction; more replicated or positively-signalled atoms appear larger.</p>
          <p><span className="text-[var(--text-primary)] font-medium">Click a node</span> to open its detail panel. You can click directly from one node to another without closing first.</p>
          <p><span className="text-[var(--text-primary)] font-medium">Edges</span> show relationships: grey = derived from, green = replicates, red = contradicts.</p>
          <p>The layout is computed with ForceAtlas2 — clusters of related atoms attract each other. The map refreshes every 30 seconds.</p>
        </div>
        <button onClick={onClose} className="w-full py-2 border border-[var(--border)] text-[var(--text-primary)] rounded-lg text-sm hover:bg-[var(--bg-subtle)] transition-colors">Got it</button>
      </div>
    </div>
  )
}

function FieldMapComponent() {
  const [selectedAtom, setSelectedAtom] = useState<Atom | null>(null)
  const [showHelp, setShowHelp] = useState(false)

  const { data: graphData, isLoading, error } = useQuery({
    queryKey: ['fieldMap'],
    queryFn: () => jsonRpcClient.getGraph(),
    refetchInterval: 30000,
  })

  const handleNodeSelect = (atomId: string) => {
    const atom = atoms.find(a => a.atom_id === atomId)
    if (atom) setSelectedAtom(atom)
  }

  if (isLoading) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        <div className="text-[var(--text-muted)]">Loading field map...</div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="w-full h-full flex items-center justify-center flex-col gap-2">
        <div className="text-[var(--danger)]">Error loading field map</div>
        <div className="text-sm text-[var(--text-muted)]">
          {error instanceof Error ? error.message : 'Unknown error'}
        </div>
      </div>
    )
  }

  const atoms = graphData?.atoms || []
  const edges = graphData?.edges || []

  return (
    <div className="w-full h-full relative">
      {showHelp && <HelpModal onClose={() => setShowHelp(false)} />}

      <FieldMap
        atoms={atoms}
        edges={edges}
        onNodeClick={setSelectedAtom}
      />
      <AtomDetailsPanel
        atom={selectedAtom}
        onClose={() => setSelectedAtom(null)}
        onNodeSelect={handleNodeSelect}
      />
      <div className="absolute top-4 left-4 z-10">
        <GraphLegend />
      </div>
      <button
        onClick={() => setShowHelp(true)}
        className="absolute bottom-4 right-4 z-10 p-2 bg-[var(--bg)] border border-[var(--border)] rounded text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors"
        title="Help"
      >
        <HelpCircle className="w-4 h-4" />
      </button>
    </div>
  )
}
