import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import type { Run, TaskMetric } from '#/lib/dashboard-utils'

interface TopRunsTableProps {
  runs: Run[]
  metric: TaskMetric
  freeParams: string[]
}

export default function TopRunsTable({ runs, metric, freeParams }: TopRunsTableProps) {
  const fmt = (v: number) => metric.unit ? `${v} ${metric.unit}` : String(+v.toFixed(6))

  // Show at most 4 free params to keep table readable
  const cols = freeParams.slice(0, 4)

  return (
    <Card>
      <CardHeader>
        <CardTitle>Top Runs — {metric.name}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-[var(--border)]">
                <th className="text-left p-2">#</th>
                <th className="text-left p-2">{metric.name}</th>
                {cols.map(k => <th key={k} className="text-left p-2">{k}</th>)}
                <th className="text-left p-2">lifecycle</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((run, i) => (
                <tr key={run.atom_id} className="border-b border-[var(--border)]">
                  <td className="p-2 font-medium text-[var(--text-muted)]">{i + 1}</td>
                  <td className="p-2 font-mono">{fmt(run.metricValues[metric.name])}</td>
                  {cols.map(k => (
                    <td key={k} className="p-2 font-mono text-xs">
                      {run.conditions[k] !== undefined ? String(run.conditions[k]) : '—'}
                    </td>
                  ))}
                  <td className="p-2">
                    <span className={`px-2 py-0.5 rounded text-xs border ${
                      run.lifecycle === 'core' || run.lifecycle === 'replicated'
                        ? 'border-[var(--accent)] text-[var(--accent)]'
                        : run.lifecycle === 'contested'
                        ? 'border-[var(--danger)] text-[var(--danger)]'
                        : 'border-[var(--text-muted)] text-[var(--text-muted)]'
                    }`}>
                      {run.lifecycle}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {runs.length === 0 && (
            <div className="text-center py-8 text-[var(--text-muted)] text-sm">
              No runs reporting <span className="font-mono">{metric.name}</span> yet
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
