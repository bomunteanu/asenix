import { useCallback, useRef, useEffect, useState, useMemo } from 'react'
import ForceGraph3D from 'react-force-graph-3d'
import type { Atom, Edge } from '#/lib/bindings'
import { getCSSVariable, getChartColors } from '#/lib/chart-utils'
import { useTheme } from '#/stores/theme'

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

// Blend `from` toward `to` by factor t: t=1 returns from, t=0 returns to.
function blendHex(from: string, to: string, t: number): string {
  const parse = (h: string): [number, number, number] => {
    const c = h.replace('#', '').slice(0, 6).padEnd(6, '0')
    return [parseInt(c.slice(0, 2), 16), parseInt(c.slice(2, 4), 16), parseInt(c.slice(4, 6), 16)]
  }
  const toHex = (n: number) => Math.round(Math.max(0, Math.min(255, n))).toString(16).padStart(2, '0')
  const [r1, g1, b1] = parse(from)
  const [r2, g2, b2] = parse(to)
  return `#${toHex(r2 + (r1 - r2) * t)}${toHex(g2 + (g1 - g2) * t)}${toHex(b2 + (b1 - b2) * t)}`
}

// Oldest atoms blend to 30% of their vivid color toward the background.
// Newest atoms stay at full vivid color.
const MIN_VIVIDNESS = 0.30

export default function FieldMap({ atoms, edges, onNodeClick, highlightedAtoms }: FieldMapProps) {
  const { theme } = useTheme()
  const containerRef = useRef<HTMLDivElement>(null)
  const graphRef = useRef<any>(null)
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

  // Re-read CSS variables whenever the theme changes.
  const bgColor = useMemo(() => getCSSVariable('--bg'), [theme])

  // ForceGraph3D only sets the renderer clear color on mount — push updates imperatively.
  useEffect(() => {
    graphRef.current?.renderer().setClearColor(bgColor)
  }, [bgColor])

  // Normalized age map: atom_id → t in [0, 1] where 1 = newest, 0 = oldest.
  const ageMap = useMemo(() => {
    const map = new Map<string, number>()
    if (atoms.length === 0) return map
    const timestamps = atoms.map(a => new Date(a.created_at).getTime())
    const minTs = Math.min(...timestamps)
    const maxTs = Math.max(...timestamps)
    const range = maxTs - minTs
    for (const a of atoms) {
      const t = range === 0 ? 1 : (new Date(a.created_at).getTime() - minTs) / range
      map.set(a.atom_id, t)
    }
    return map
  }, [atoms])

  const graphData = useMemo(() => ({
    nodes: atoms.map(atom => ({ id: atom.atom_id, atom })) as GraphNode[],
    links: edges
      .filter(e => atoms.some(a => a.atom_id === e.source_id) && atoms.some(a => a.atom_id === e.target_id))
      .map(e => ({ source: e.source_id, target: e.target_id, edge_type: e.edge_type })) as GraphLink[],
  }), [atoms, edges])

  const nodeColor = useCallback((node: object) => {
    const n = node as GraphNode
    if (highlightedAtoms?.has(n.atom.atom_id)) return '#facc15'
    const c = getChartColors()
    const typeColors: Record<string, string> = {
      bounty: c.nodeBounty,
      finding: c.nodeFinding,
      hypothesis: c.nodeHypothesis,
      negative_result: c.nodeNegativeResult,
      synthesis: c.nodeSynthesis,
    }
    const vivid = typeColors[n.atom.atom_type] ?? c.textMuted
    const tNorm = ageMap.get(n.atom.atom_id) ?? 1
    const t = MIN_VIVIDNESS + (1 - MIN_VIVIDNESS) * tNorm
    return blendHex(vivid, bgColor, t)
  }, [highlightedAtoms, ageMap, bgColor])

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
    return map[l.edge_type] ?? (theme === 'dark' ? '#ffffff22' : '#00000022')
  }, [theme])

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
        ref={graphRef}
        graphData={graphData}
        width={dimensions.width}
        height={dimensions.height}
        backgroundColor={bgColor}
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
