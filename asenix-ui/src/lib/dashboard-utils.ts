import type { Atom } from '#/lib/bindings'

// A task is derived from a bounty atom that declares metrics + free parameters
export interface Task {
  bounty_id: string
  domain: string
  statement: string
  metrics: TaskMetric[]
  freeParams: string[]            // condition keys set to null  → vary these
  fixedParams: Record<string, any> // condition keys with values → fixed context
}

export interface TaskMetric {
  name: string
  direction: 'maximize' | 'minimize'
  unit?: string
}

export interface Run {
  atom_id: string
  atom_type: string
  lifecycle: string
  time_index: number
  metricValues: Record<string, number>
  conditions: Record<string, any>
}

export interface BestPoint {
  time_index: number
  best: number
}

export interface DomainStats {
  total: number
  findings: number
  hypotheses: number
  contested: number
  bounties: number
}

// Pull all bounty atoms that have a non-empty metrics array with direction info
export function extractTasks(atoms: Atom[]): Task[] {
  return atoms
    .filter(a => a.atom_type === 'bounty' && Array.isArray(a.metrics) && (a.metrics as any[]).length > 0)
    .map(a => {
      const cond: Record<string, any> = a.conditions ?? {}
      const freeParams = Object.entries(cond).filter(([, v]) => v === null).map(([k]) => k)
      const fixedParams = Object.fromEntries(Object.entries(cond).filter(([, v]) => v !== null))
      return {
        bounty_id: a.atom_id,
        domain: a.domain,
        statement: a.statement,
        metrics: (a.metrics as any[]).map(m => ({
          name: m.name as string,
          direction: (m.direction === 'minimize' || m.direction === 'lower_better' || m.direction === 'lower'
            ? 'minimize' : 'maximize') as 'maximize' | 'minimize',
          unit: m.unit as string | undefined,
        })),
        freeParams,
        fixedParams,
      }
    })
}

// All finding/negative_result atoms in the task's domain that report at least one tracked metric
export function getRunsForTask(task: Task, atoms: Atom[]): Run[] {
  const metricNames = new Set(task.metrics.map(m => m.name))
  return atoms
    .filter(a =>
      a.domain === task.domain &&
      (a.atom_type === 'finding' || a.atom_type === 'negative_result') &&
      Array.isArray(a.metrics) &&
      (a.metrics as any[]).some((m: any) => metricNames.has(m.name))
    )
    .sort((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime())
    .map((a, i) => ({
      atom_id: a.atom_id,
      atom_type: a.atom_type,
      lifecycle: a.lifecycle,
      time_index: i,
      metricValues: Object.fromEntries(
        (a.metrics as any[])
          .filter((m: any) => metricNames.has(m.name) && typeof m.value === 'number')
          .map((m: any) => [m.name as string, m.value as number])
      ),
      conditions: (a.conditions as Record<string, any>) ?? {},
    }))
}

// Running best value over time for a single metric
export function getBestOverTime(runs: Run[], metric: TaskMetric): BestPoint[] {
  const sorted = [...runs]
    .filter(r => r.metricValues[metric.name] !== undefined)
    .sort((a, b) => a.time_index - b.time_index)

  const points: BestPoint[] = []
  let best: number | null = null
  for (const run of sorted) {
    const v = run.metricValues[metric.name]
    if (v === undefined) continue
    if (best === null ||
      (metric.direction === 'maximize' && v > best) ||
      (metric.direction === 'minimize' && v < best)
    ) {
      best = v
    }
    points.push({ time_index: run.time_index, best: best! })
  }
  return points
}

export function getTopRuns(runs: Run[], metric: TaskMetric, limit = 10): Run[] {
  return [...runs]
    .filter(r => r.metricValues[metric.name] !== undefined)
    .sort((a, b) => {
      const va = a.metricValues[metric.name] ?? 0
      const vb = b.metricValues[metric.name] ?? 0
      return metric.direction === 'maximize' ? vb - va : va - vb
    })
    .slice(0, limit)
}

export function getDomainStats(task: Task, allAtoms: Atom[]): DomainStats {
  const d = allAtoms.filter(a => a.domain === task.domain)
  return {
    total: d.length,
    findings: d.filter(a => a.atom_type === 'finding').length,
    hypotheses: d.filter(a => a.atom_type === 'hypothesis').length,
    contested: d.filter(a => a.lifecycle === 'contested').length,
    bounties: d.filter(a => a.atom_type === 'bounty').length,
  }
}
