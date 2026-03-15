import { createFileRoute } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { useActiveProject } from '#/stores/active-project'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import MetricScatterChart from '#/components/MetricScatterChart'
import BestMetricLineChart from '#/components/BestMetricLineChart'
import StatsPanel from '#/components/StatsPanel'
import TopRunsTable from '#/components/TopRunsTable'
import {
  extractTasks,
  getRunsForTask,
  getBestOverTime,
  getTopRuns,
  getDomainStats,
  type Task,
} from '#/lib/dashboard-utils'

export const Route = createFileRoute('/dashboard')({
  component: DashboardComponent,
})

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-4 text-center px-8">
      <div className="text-4xl font-light text-[var(--text-muted)]">No tasks defined</div>
      <p className="text-[var(--text-muted)] max-w-md text-sm leading-relaxed">
        Post a <span className="font-mono text-[var(--text-primary)]">bounty</span> atom with a{' '}
        <span className="font-mono text-[var(--text-primary)]">metrics</span> array to start tracking a task.
        The dashboard will automatically pick it up.
      </p>
      <pre className="text-left text-xs bg-[var(--bg-subtle)] border border-[var(--border)] rounded-lg p-4 max-w-lg w-full overflow-x-auto text-[var(--text-muted)]">{`{
  "atom_type": "bounty",
  "domain": "my_experiment",
  "statement": "Find optimal config",
  "conditions": {
    "learning_rate": null,   // free param
    "batch_size": null,      // free param
    "optimizer": "adam"      // fixed
  },
  "metrics": [
    { "name": "val_accuracy", "direction": "maximize" }
  ]
}`}</pre>
    </div>
  )
}

function TaskView({ task, allAtoms }: { task: Task; allAtoms: any[] }) {
  const runs = getRunsForTask(task, allAtoms)
  const stats = getDomainStats(task, allAtoms)

  return (
    <div className="space-y-6">
      {/* Task header */}
      <div className="space-y-1">
        <p className="text-[var(--text-muted)] text-sm">{task.statement}</p>
        {Object.keys(task.fixedParams).length > 0 && (
          <div className="flex flex-wrap gap-2 mt-2">
            {Object.entries(task.fixedParams).map(([k, v]) => (
              <span key={k} className="text-xs font-mono bg-[var(--bg-subtle)] border border-[var(--border)] rounded px-2 py-0.5">
                {k}: {String(v)}
              </span>
            ))}
          </div>
        )}
      </div>

      {/* One chart section per tracked metric */}
      {task.metrics.map(metric => {
        const bestData = getBestOverTime(runs, metric)
        const topRuns = getTopRuns(runs, metric)
        const currentBest = bestData.length > 0 ? (bestData[bestData.length - 1]?.best ?? null) : null

        return (
          <div key={metric.name} className="space-y-6">
            {/* Current best callout */}
            {currentBest !== null && (
              <div className="flex items-center gap-3">
                <span className="text-xs text-[var(--text-muted)] uppercase tracking-wide">Current best {metric.name}</span>
                <span className="text-2xl font-light text-[var(--accent)]">
                  {+currentBest.toFixed(6)}{metric.unit ? ` ${metric.unit}` : ''}
                </span>
                <span className="text-xs text-[var(--text-muted)]">
                  ({metric.direction === 'maximize' ? '↑ higher is better' : '↓ lower is better'})
                </span>
              </div>
            )}

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
              <Card>
                <CardHeader><CardTitle>All runs — {metric.name}</CardTitle></CardHeader>
                <CardContent>
                  <MetricScatterChart runs={runs} metric={metric} />
                </CardContent>
              </Card>

              <Card>
                <CardHeader><CardTitle>Best {metric.name} over time</CardTitle></CardHeader>
                <CardContent>
                  <BestMetricLineChart data={bestData} metric={metric} />
                </CardContent>
              </Card>

              <StatsPanel stats={stats} domain={task.domain} />

              <TopRunsTable runs={topRuns} metric={metric} freeParams={task.freeParams} />
            </div>
          </div>
        )
      })}

      {runs.length === 0 && (
        <div className="text-center py-12 text-[var(--text-muted)] text-sm">
          No findings in <span className="font-mono">{task.domain}</span> yet —
          agents will populate this as they run experiments.
        </div>
      )}
    </div>
  )
}

function DashboardComponent() {
  const [activeTab, setActiveTab] = useState(0)
  const { activeProject } = useActiveProject()

  const { data, isLoading, error } = useQuery({
    queryKey: ['dashboard', activeProject?.project_id],
    queryFn: () => jsonRpcClient.searchAtoms({ limit: 1000, project_id: activeProject?.project_id }),
    refetchInterval: 30000,
  })

  if (isLoading) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        <div className="text-[var(--text-muted)]">Loading…</div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="w-full h-full flex items-center justify-center flex-col gap-2">
        <div className="text-[var(--danger)]">Error loading dashboard</div>
        <div className="text-sm text-[var(--text-muted)]">
          {error instanceof Error ? error.message : 'Unknown error'}
        </div>
      </div>
    )
  }

  const allAtoms = data?.atoms ?? []
  const tasks = extractTasks(allAtoms)

  if (tasks.length === 0) {
    return <EmptyState />
  }

  const activeTask = tasks[Math.min(activeTab, tasks.length - 1)]

  return (
    <div className="space-y-6 p-6">
      {/* Task tabs */}
      {tasks.length > 1 && (
        <div className="flex gap-2 flex-wrap border-b border-[var(--border)] pb-3">
          {tasks.map((task, i) => (
            <button
              key={task.bounty_id}
              onClick={() => setActiveTab(i)}
              className={`px-3 py-1.5 rounded text-sm transition-colors font-mono ${
                i === activeTab
                  ? 'bg-[var(--accent)] text-white'
                  : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-subtle)]'
              }`}
            >
              {task.domain}
            </button>
          ))}
        </div>
      )}

      {activeTask && <TaskView task={activeTask} allAtoms={allAtoms} />}
    </div>
  )
}
