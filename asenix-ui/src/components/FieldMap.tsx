import { useEffect, useRef, useState } from 'react'
import Sigma from 'sigma'
import Graph from 'graphology'
import useLayout from 'graphology-layout-forceatlas2'
import type { Atom, Edge } from '#/lib/bindings'
import { getChartColors } from '#/lib/chart-utils'
import { useTheme } from '#/stores/theme'

interface FieldMapProps {
  atoms: Atom[]
  edges: Edge[]
  onNodeClick: (atom: Atom) => void
}

export default function FieldMap({ atoms, edges, onNodeClick }: FieldMapProps) {
  const theme = useTheme((state) => state.theme)
  const containerRef = useRef<HTMLDivElement>(null)
  const sigmaRef = useRef<Sigma | null>(null)
  const [selectedNode, setSelectedNode] = useState<string | null>(null)

  const [nodeColors, setNodeColors] = useState<Record<string, string>>({
    'bounty': '#2d6a4f',
    'finding': '#2563eb',
    'hypothesis': '#7c3aed',
    'negative_result': '#dc2626',
    'synthesis': '#ea580c',
  })

  const [edgeColors, setEdgeColors] = useState<Record<string, string>>({
    'derived_from': '#9ca3af',
    'contradicts': '#dc2626',
    'replicates': '#059669',
  })

  useEffect(() => {
    const chartColors = getChartColors()
    setNodeColors({
      'bounty': chartColors.nodeBounty,
      'finding': chartColors.nodeFinding,
      'hypothesis': chartColors.nodeHypothesis,
      'negative_result': chartColors.nodeNegativeResult,
      'synthesis': chartColors.nodeSynthesis,
    })
    setEdgeColors({
      'derived_from': chartColors.edgeDerived,
      'contradicts': chartColors.edgeContradicts,
      'replicates': chartColors.edgeReplicates,
    })
  }, [theme])

  useEffect(() => {
    if (!containerRef.current || atoms.length === 0) return

    // Create graph
    const graph = new Graph({ multi: true })

    // Add nodes — circular seed positions so ForceAtlas2 converges consistently
    atoms.forEach((atom, index) => {
      const valAccuracy = atom.metrics?.find((m: any) => m.name === 'val_accuracy')
      const phAttraction = atom.ph_attraction ?? 0
      const angle = (2 * Math.PI * index) / Math.max(atoms.length, 1)
      const radius = 100

      graph.addNode(atom.atom_id, {
        label: atom.atom_type,
        size: Math.max(8, Math.min(25, 8 + phAttraction * 17)),
        color: nodeColors[atom.atom_type] || (theme === 'dark' ? '#6b6860' : '#6b6860'),
        atom,
        valAccuracy,
        x: Math.cos(angle) * radius,
        y: Math.sin(angle) * radius,
      })
    })

    // Add edges
    edges.forEach((edge) => {
      if (graph.hasNode(edge.source_id) && graph.hasNode(edge.target_id)) {
        graph.addEdge(edge.source_id, edge.target_id, {
          color: edgeColors[edge.edge_type] || (theme === 'dark' ? '#6b7280' : '#9ca3af'),
          size: 2,
          edge,
        })
      }
    })

    // Create Sigma instance
    const sigma = new Sigma(graph, containerRef.current, {
      renderLabels: false,
      defaultNodeColor: theme === 'dark' ? '#6b6860' : '#6b6860',
      defaultEdgeColor: theme === 'dark' ? '#6b7280' : '#9ca3af',
      minCameraRatio: 0.1,
      maxCameraRatio: 10,
    })

    // Apply ForceAtlas2 layout — deterministic (barnesHutOptimize off) + more iterations
    const positions = useLayout(graph, {
      iterations: 150,
      settings: {
        gravity: 1.5,
        linLogMode: true,
        strongGravityMode: false,
        barnesHutOptimize: false,
        scalingRatio: 1.2,
        adjustSizes: true,
      },
    })

    // Apply layout positions to nodes
    graph.forEachNode((node) => {
      const position = positions[node]
      if (position) {
        graph.setNodeAttribute(node, 'x', position.x)
        graph.setNodeAttribute(node, 'y', position.y)
      }
    })

    // Handle node hover
    sigma.on('enterNode', ({ node }) => {
      setSelectedNode(node)
      const graphNode = graph.getNodeAttributes(node)
      sigma.getGraph().setNodeAttribute(node, 'size', (graphNode.size as number) * 1.5)
    })

    sigma.on('leaveNode', ({ node }) => {
      setSelectedNode(null)
      const graphNode = graph.getNodeAttributes(node)
      sigma.getGraph().setNodeAttribute(node, 'size', (graphNode.size as number) / 1.5)
    })

    // Handle node click
    sigma.on('clickNode', ({ node }) => {
      const atom = graph.getNodeAttributes(node).atom as Atom
      onNodeClick(atom)
    })

    sigmaRef.current = sigma

    return () => {
      if (sigmaRef.current) {
        sigmaRef.current.kill()
        sigmaRef.current = null
      }
    }
  }, [atoms, edges, onNodeClick])

  const handleZoomIn = () => {
    if (sigmaRef.current) {
      sigmaRef.current.getCamera().animatedZoom({ duration: 300 })
    }
  }

  const handleZoomOut = () => {
    if (sigmaRef.current) {
      sigmaRef.current.getCamera().animatedUnzoom({ duration: 300 })
    }
  }

  const handleResetView = () => {
    if (sigmaRef.current) {
      sigmaRef.current.getCamera().animatedReset({ duration: 300 })
    }
  }

  return (
    <div className="relative w-full h-full">
      <div ref={containerRef} className="w-full h-full" />
      
      {/* Controls */}
      <div className="absolute top-4 right-4 flex flex-col gap-2">
        <button
          onClick={handleZoomIn}
          className="bg-[var(--bg)] border border-[var(--border)] rounded p-2 text-[var(--text-primary)] hover:bg-[var(--bg-subtle)] transition-colors"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"/>
          </svg>
        </button>
        <button
          onClick={handleZoomOut}
          className="bg-[var(--bg)] border border-[var(--border)] rounded p-2 text-[var(--text-primary)] hover:bg-[var(--bg-subtle)] transition-colors"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M19 13H5v-2h14v2z"/>
          </svg>
        </button>
        <button
          onClick={handleResetView}
          className="bg-[var(--bg)] border border-[var(--border)] rounded p-2 text-[var(--text-primary)] hover:bg-[var(--bg-subtle)] transition-colors"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 5V1L7 6l5 5V7c3.31 0 6 2.69 6 6s-2.69 6-6 6-6-2.69-6-6 2.69-6 6-6h2c-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4-1.79-4-4-4z"/>
          </svg>
        </button>
      </div>

      {/* Tooltip */}
      {selectedNode && (
        <div className="absolute bottom-4 left-4 bg-[var(--bg)] border border-[var(--border)] rounded p-3 max-w-sm shadow-sm">
          <div className="text-sm">
            <div className="font-medium text-[var(--text-primary)] mb-1">
              {atoms.find(a => a.atom_id === selectedNode)?.atom_type}
            </div>
            <div className="text-[var(--text-muted)] mb-2">
              {atoms.find(a => a.atom_id === selectedNode)?.statement.slice(0, 100)}...
            </div>
            <div className="flex items-center gap-2">
              {(() => {
                const atom = atoms.find(a => a.atom_id === selectedNode)
                const valAccuracy = atom?.metrics?.find((m: any) => m.name === 'val_accuracy')
                return valAccuracy ? (
                  <span className="text-xs bg-[var(--accent)] text-white px-2 py-1 rounded">
                    {(valAccuracy.value * 100).toFixed(2)}%
                  </span>
                ) : null
              })()}
              <span className={`text-xs px-2 py-1 rounded border ${
                atoms.find(a => a.atom_id === selectedNode)?.lifecycle === 'replicated'
                  ? 'border-[var(--accent)] text-[var(--accent)]'
                  : atoms.find(a => a.atom_id === selectedNode)?.lifecycle === 'contested'
                  ? 'border-[var(--danger)] text-[var(--danger)]'
                  : 'border-[var(--text-muted)] text-[var(--text-muted)]'
              }`}>
                {atoms.find(a => a.atom_id === selectedNode)?.lifecycle}
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
