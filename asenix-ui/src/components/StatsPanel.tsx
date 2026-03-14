import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import type { DashboardStats } from '#/lib/dashboard-utils'

interface StatsPanelProps {
  stats: DashboardStats
}

export default function StatsPanel({ stats }: StatsPanelProps) {
  const statItems = [
    { label: 'Total Atoms', value: stats.totalAtoms },
    { label: 'Training Runs', value: stats.trainingRuns },
    { label: 'Contradictions', value: stats.contradictions },
    { label: 'Bounties', value: stats.bounties },
    { label: 'Hypotheses', value: stats.hypotheses },
  ]

  return (
    <Card>
      <CardHeader>
        <CardTitle>Statistics</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 gap-4">
          {statItems.map((item, index) => (
            <div key={index} className="text-center p-4 bg-[var(--bg)] rounded-lg border border-[var(--border)]">
              <div className="text-3xl font-light text-[var(--text-primary)]">
                {item.value}
              </div>
              <div className="all-caps mt-1">
                {item.label}
              </div>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  )
}
