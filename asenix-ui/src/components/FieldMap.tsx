import { useCallback, useRef, useEffect, useState } from 'react'
import ForceGraph3D from 'react-force-graph-3d'
import type { Atom, Edge } from '#/lib/bindings'
import { getChartColors } from '#/lib/chart-utils'

interface FieldMapProps {
  atoms: Atom[]
  edges: Edge[]
  embeddings: Record<string, number[]>
  onNodeClick: (atom: Atom) => void
  highlightedAtoms?: Set<string>
}

interface GraphNode {
  id: string
  atom: Atom
}

interface GraphLink {
  source: string
  target: string
  edge_type: string
}

export default function FieldMap({ atoms, edges, onNodeClick, highlightedAtoms }: FieldMapProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const [dimensions, setDimensions] = useState({ width: 800, height: 600 })

  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const obs = new ResizeObserver(entries => {
      const e = entries[0]
      if (e) setDimensions({ width: e.contentRect.width, height: e.contentRect.height })
    })
    obs.observe(el)
    setDimensions({ width: el.clientWidth, height: el.clientHeight })
    return () => obs.disconnect()
  }, [])

  const graphData = {
    nodes: atoms.map(atom => ({ id: atom.atom_id, atom })) as GraphNode[],
    links: edges
      .filter(e => atoms.some(a => a.atom_id === e.source_id) && atoms.some(a => a.atom_id === e.target_id))
      .map(e => ({ source: e.source_id, target: e.target_id, edge_type: e.edge_type })) as GraphLink[],
  }

  const nodeColor = useCallback((node: object) => {
    const n = node as GraphNode
    if (highlightedAtoms?.has(n.atom.atom_id)) return '#facc15'
    const c = getChartColors()
    const map: Record<string, string> = {
      bounty: c.nodeBounty,
      finding: c.nodeFinding,
      hypothesis: c.nodeHypothesis,
      negative_result: c.nodeNegativeResult,
      synthesis: c.nodeSynthesis,
    }
    return map[n.atom.atom_type] ?? c.textMuted
  }, [highlightedAtoms])

  const nodeVal = useCallback((node: object) => {
    const n = node as GraphNode
    return Math.max(5, Math.min(25, (n.atom.ph_attraction ?? 0) * 10 + 5))
  }, [])

  const nodeLabel = useCallback((node: object) => {
    const n = node as GraphNode
    return n.atom.statement.slice(0, 80)
  }, [])

  const linkColor = useCallback((link: object) => {
    const l = link as GraphLink
    const c = getChartColors()
    const map: Record<string, string> = {
      derived_from: c.edgeDerived,
      replicates: c.edgeReplicates,
      contradicts: c.edgeContradicts,
    }
    return map[l.edge_type] ?? '#ffffff22'
  }, [])

  const linkWidth = useCallback((link: object) => {
    const l = link as GraphLink
    return (l.edge_type === 'replicates' || l.edge_type === 'contradicts') ? 2 : 1
  }, [])

  const handleNodeClick = useCallback((node: object) => {
    const n = node as GraphNode
    onNodeClick(n.atom)
  }, [onNodeClick])

  return (
    <div ref={containerRef} className="w-full h-full">
      <ForceGraph3D
        graphData={graphData}
        width={dimensions.width}
        height={dimensions.height}
        backgroundColor="#0a0a0a"
        nodeColor={nodeColor}
        nodeVal={nodeVal}
        nodeLabel={nodeLabel}
        linkColor={linkColor}
        linkWidth={linkWidth}
        linkOpacity={0.5}
        onNodeClick={handleNodeClick}
        nodeAutoColorBy={undefined}
      />
    </div>
  )
}
