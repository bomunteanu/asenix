import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import type { TopRun } from '#/lib/dashboard-utils'

interface TopRunsTableProps {
  runs: TopRun[]
}

export default function TopRunsTable({ runs }: TopRunsTableProps) {
  const formatLearningRate = (lr?: number) => {
    if (lr === undefined) return 'N/A'
    if (lr < 0.001) return `${lr * 1000000}μ`
    if (lr < 1) return `${lr * 1000}m`
    return lr.toString()
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Top Training Runs</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-[var(--line)]">
                <th className="text-left p-2">Rank</th>
                <th className="text-left p-2">Val Acc</th>
                <th className="text-left p-2">Val Loss</th>
                <th className="text-left p-2">Optimizer</th>
                <th className="text-left p-2">Scheduler</th>
                <th className="text-left p-2">LR</th>
                <th className="text-left p-2">Lifecycle</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((run) => (
                <tr 
                  key={run.atom_id}
                  className="border-b border-[var(--border)]"
                >
                  <td className="p-2 font-medium">{run.rank}</td>
                  <td className="p-2 font-mono">
                    {(run.val_accuracy * 100).toFixed(2)}%
                  </td>
                  <td className="p-2 font-mono">
                    {run.val_loss?.toFixed(6) || 'N/A'}
                  </td>
                  <td className="p-2">{run.optimizer}</td>
                  <td className="p-2">{run.scheduler}</td>
                  <td className="p-2 font-mono text-xs">
                    {formatLearningRate(run.learning_rate)}
                  </td>
                  <td className="p-2">
                    <span className={`px-2 py-1 rounded text-xs border ${
                      run.lifecycle === 'replicated' 
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
            <div className="text-center py-8 text-[var(--text-muted)]">
              No training runs with validation accuracy found
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
