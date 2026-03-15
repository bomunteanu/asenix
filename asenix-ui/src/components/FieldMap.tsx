import { useEffect, useRef } from 'react'
import Sigma from 'sigma'
import Graph from 'graphology'
import forceAtlas2 from 'graphology-layout-forceatlas2'
import { UMAP } from 'umap-js'
import type { Atom, Edge } from '#/lib/bindings'
import { getChartColors } from '#/lib/chart-utils'
import { useTheme } from '#/stores/theme'

interface FieldMapProps {
  atoms: Atom[]
  edges: Edge[]
  embeddings: Record<string, number[]>   // atom_id → 384-dim vector
  onNodeClick: (atom: Atom) => void
  highlightedAtoms?: Set<string>         // atom_ids to visually highlight (recently published)
}

// Compute 2-D positions from embedding vectors using UMAP.
// Atoms without an embedding fall back to a circle on the periphery.
function computePositions(
  atoms: Atom[],
  embeddings: Record<string, number[]>,
): Record<string, { x: number; y: number }> {
  const withEmb = atoms.filter(a => embeddings[a.atom_id])
  const withoutEmb = atoms.filter(a => !embeddings[a.atom_id])

  const positions: Record<string, { x: number; y: number }> = {}

  if (withEmb.length >= 4) {
    const matrix = withEmb.map(a => embeddings[a.atom_id] as number[])

    // Seeded PRNG (mulberry32) so UMAP produces the same layout for the same data
    const seed = 0xdeadbeef
    let s = seed
    const seededRandom = () => { s |= 0; s = s + 0x6d2b79f5 | 0; let t = Math.imul(s ^ s >>> 15, 1 | s); t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 4294967296 }

    const umap = new UMAP({
      nComponents: 2,
      nNeighbors: Math.min(15, withEmb.length - 1),
      minDist: 0.1,
      spread: 1.0,
      random: seededRandom,
    })
    const coords = umap.fit(matrix) as number[][] // number[][] shape [n, 2]

    // Normalise to [-100, 100]
    const xs = coords.map(c => c[0] as number)
    const ys = coords.map(c => c[1] as number)
    const minX = Math.min(...xs), maxX = Math.max(...xs)
    const minY = Math.min(...ys), maxY = Math.max(...ys)
    const rangeX = maxX - minX || 1
    const rangeY = maxY - minY || 1

    withEmb.forEach((atom, i) => {
      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      const row = coords[i]!
      positions[atom.atom_id] = {
        x: ((row[0]! - minX) / rangeX - 0.5) * 200,
        y: ((row[1]! - minY) / rangeY - 0.5) * 200,
      }
    })
  } else {
    // Not enough embeddings yet — fall back to circle for all
    atoms.forEach((atom, i) => {
      const angle = (2 * Math.PI * i) / Math.max(atoms.length, 1)
      positions[atom.atom_id] = { x: Math.cos(angle) * 100, y: Math.sin(angle) * 100 }
    })
    return positions
  }

  // Unembedded atoms go on an outer ring
  withoutEmb.forEach((atom, i) => {
    const angle = (2 * Math.PI * i) / Math.max(withoutEmb.length, 1)
    positions[atom.atom_id] = { x: Math.cos(angle) * 260, y: Math.sin(angle) * 260 }
  })

  return positions
}

export default function FieldMap({ atoms, edges, embeddings, onNodeClick, highlightedAtoms }: FieldMapProps) {
  const theme = useTheme(state => state.theme)
  const containerRef = useRef<HTMLDivElement>(null)
  const sigmaRef = useRef<Sigma | null>(null)
  const hoverNodeRef = useRef<string | null>(null)
  // Ref so the main build effect can read the current highlight set without depending on it
  const highlightedAtomsRef = useRef(highlightedAtoms)
  highlightedAtomsRef.current = highlightedAtoms

  useEffect(() => {
    if (!containerRef.current || atoms.length === 0) return

    const chartColors = getChartColors()

    const nodeColors: Record<string, string> = {
      bounty: chartColors.nodeBounty,
      finding: chartColors.nodeFinding,
      hypothesis: chartColors.nodeHypothesis,
      negative_result: chartColors.nodeNegativeResult,
      synthesis: chartColors.nodeSynthesis,
    }
    const edgeColors: Record<string, string> = {
      derived_from: chartColors.edgeDerived,
      contradicts: chartColors.edgeContradicts,
      replicates: chartColors.edgeReplicates,
    }
    const fallbackNode = chartColors.textMuted
    const fallbackEdge = theme === 'dark' ? '#6b7280' : '#9ca3af'

    // Compute UMAP positions (sync — takes ~0.5–2s for typical graph sizes)
    const positions = computePositions(atoms, embeddings)

    const graph = new Graph({ multi: true })

    atoms.forEach(atom => {
      const pos = positions[atom.atom_id] ?? { x: 0, y: 0 }
      const isHighlighted = highlightedAtomsRef.current?.has(atom.atom_id) ?? false
      graph.addNode(atom.atom_id, {
        label: atom.atom_type,
        size: Math.max(8, Math.min(25, 8 + (atom.ph_attraction ?? 0) * 17)) + (isHighlighted ? 4 : 0),
        color: isHighlighted ? '#facc15' : (nodeColors[atom.atom_type] ?? fallbackNode),
        atom,
        x: pos.x,
        y: pos.y,
      })
    })

    edges.forEach(edge => {
      if (graph.hasNode(edge.source_id) && graph.hasNode(edge.target_id)) {
        graph.addEdge(edge.source_id, edge.target_id, {
          color: edgeColors[edge.edge_type] ?? fallbackEdge,
          size: 2,
        })
      }
    })

    // Light ForceAtlas2 refinement — respects edges while keeping UMAP clusters intact
    const hasEmbeddings = Object.keys(embeddings).length > 0
    forceAtlas2.assign(graph, {
      iterations: hasEmbeddings ? 50 : 150,
      settings: {
        gravity: hasEmbeddings ? 0.5 : 1.5,
        linLogMode: true,
        strongGravityMode: false,
        barnesHutOptimize: false,
        scalingRatio: hasEmbeddings ? 0.8 : 1.2,
        adjustSizes: true,
      },
    })

    const sigma = new Sigma(graph, containerRef.current, {
      renderLabels: false,
      defaultNodeColor: fallbackNode,
      defaultEdgeColor: fallbackEdge,
      minCameraRatio: 0.05,
      maxCameraRatio: 10,
    })

    sigma.on('enterNode', ({ node }) => {
      hoverNodeRef.current = node
      const attrs = graph.getNodeAttributes(node)
      graph.setNodeAttribute(node, 'size', (attrs.size as number) * 1.5)
    })

    sigma.on('leaveNode', ({ node }) => {
      hoverNodeRef.current = null
      const attrs = graph.getNodeAttributes(node)
      graph.setNodeAttribute(node, 'size', (attrs.size as number) / 1.5)
    })

    sigma.on('clickNode', ({ node }) => {
      const atom = graph.getNodeAttributes(node).atom as Atom
      onNodeClick(atom)
    })

    sigmaRef.current = sigma

    return () => {
      sigmaRef.current?.kill()
      sigmaRef.current = null
    }
  }, [atoms, edges, embeddings, onNodeClick, theme])

  // Second effect: update highlight state without rebuilding Sigma.
  // Kept separate so UMAP+ForceAtlas2 is not re-run each time a highlight expires.
  useEffect(() => {
    if (!sigmaRef.current) return
    const graph = sigmaRef.current.getGraph()
    const chartColors = getChartColors()
    const nodeColors: Record<string, string> = {
      bounty: chartColors.nodeBounty,
      finding: chartColors.nodeFinding,
      hypothesis: chartColors.nodeHypothesis,
      negative_result: chartColors.nodeNegativeResult,
      synthesis: chartColors.nodeSynthesis,
    }
    const fallbackNode = chartColors.textMuted

    graph.forEachNode(nodeId => {
      const atom = graph.getNodeAttributes(nodeId).atom as Atom
      const isHighlighted = highlightedAtoms?.has(atom.atom_id) ?? false
      const baseSize = Math.max(8, Math.min(25, 8 + (atom.ph_attraction ?? 0) * 17))
      graph.setNodeAttribute(nodeId, 'size', baseSize + (isHighlighted ? 4 : 0))
      graph.setNodeAttribute(nodeId, 'color',
        isHighlighted ? '#facc15' : (nodeColors[atom.atom_type] ?? fallbackNode))
    })
    sigmaRef.current.refresh()
  }, [highlightedAtoms])

  const camera = () => sigmaRef.current?.getCamera()

  return (
    <div className="relative w-full h-full">
      <div ref={containerRef} className="w-full h-full" />

      {/* Zoom controls */}
      <div className="absolute top-4 right-4 flex flex-col gap-2">
        {[
          { title: 'Zoom in',  onClick: () => camera()?.animatedZoom({ duration: 300 }),   path: 'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z' },
          { title: 'Zoom out', onClick: () => camera()?.animatedUnzoom({ duration: 300 }), path: 'M19 13H5v-2h14v2z' },
          { title: 'Reset',    onClick: () => camera()?.animatedReset({ duration: 300 }),   path: 'M12 5V1L7 6l5 5V7c3.31 0 6 2.69 6 6s-2.69 6-6 6-6-2.69-6-6H4c0 4.42 3.58 8 8 8s8-3.58 8-8-3.58-8-8-8z' },
        ].map(btn => (
          <button
            key={btn.title}
            onClick={btn.onClick}
            title={btn.title}
            className="bg-[var(--bg)] border border-[var(--border)] rounded p-2 text-[var(--text-primary)] hover:bg-[var(--bg-subtle)] transition-colors"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <path d={btn.path} />
            </svg>
          </button>
        ))}
      </div>
    </div>
  )
}
