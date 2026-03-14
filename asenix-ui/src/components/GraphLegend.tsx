import { useState } from 'react'
import { ChevronDown, ChevronRight } from 'lucide-react'

interface GraphLegendProps {
  className?: string
}

export default function GraphLegend({ className = '' }: GraphLegendProps) {
  const [isExpanded, setIsExpanded] = useState(false)

  const nodeTypes = [
    { type: 'bounty', color: 'var(--node-bounty)', label: 'Bounty' },
    { type: 'finding', color: 'var(--node-finding)', label: 'Finding' },
    { type: 'hypothesis', color: 'var(--node-hypothesis)', label: 'Hypothesis' },
    { type: 'negative_result', color: 'var(--node-negative-result)', label: 'Negative Result' },
    { type: 'synthesis', color: 'var(--node-synthesis)', label: 'Synthesis' },
  ]

  const edgeTypes = [
    { type: 'derived_from', color: 'var(--edge-derived)', label: 'Derived From' },
    { type: 'contradicts', color: 'var(--edge-contradicts)', label: 'Contradicts' },
    { type: 'replicates', color: 'var(--edge-replicates)', label: 'Replicates' },
  ]

  return (
    <div className={`bg-[var(--bg)] border border-[var(--border)] rounded-lg shadow-sm ${className}`}>
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="w-full px-4 py-3 flex items-center justify-between text-[var(--text-primary)] hover:bg-[var(--bg-subtle)] transition-colors"
      >
        <span className="font-medium">Graph Legend</span>
        {isExpanded ? (
          <ChevronDown className="w-4 h-4 text-[var(--text-muted)]" />
        ) : (
          <ChevronRight className="w-4 h-4 text-[var(--text-muted)]" />
        )}
      </button>
      
      {isExpanded && (
        <div className="px-4 pb-4 space-y-4">
          <div>
            <h4 className="text-sm font-medium text-[var(--text-primary)] mb-2">Node Types</h4>
            <div className="space-y-1">
              {nodeTypes.map(({ type, color, label }) => (
                <div key={type} className="flex items-center gap-2">
                  <div 
                    className="w-3 h-3 rounded-full" 
                    style={{ backgroundColor: color }}
                  />
                  <span className="text-xs text-[var(--text-muted)]">{label}</span>
                </div>
              ))}
            </div>
          </div>
          
          <div>
            <h4 className="text-sm font-medium text-[var(--text-primary)] mb-2">Edge Types</h4>
            <div className="space-y-1">
              {edgeTypes.map(({ type, color, label }) => (
                <div key={type} className="flex items-center gap-2">
                  <div 
                    className="w-8 h-0.5" 
                    style={{ backgroundColor: color }}
                  />
                  <span className="text-xs text-[var(--text-muted)]">{label}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
