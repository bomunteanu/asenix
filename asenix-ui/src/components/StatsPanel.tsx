import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import type { DomainStats } from '#/lib/dashboard-utils'

interface StatsPanelProps {
  stats: DomainStats
  domain: string
}

export default function StatsPanel({ stats, domain }: StatsPanelProps) {
  const items = [
    { label: 'Total Atoms', value: stats.total },
    { label: 'Findings', value: stats.findings },
    { label: 'Hypotheses', value: stats.hypotheses },
    { label: 'Contested', value: stats.contested },
    { label: 'Bounties', value: stats.bounties },
  ]

  return (
    <Card>
      <CardHeader>
        <CardTitle>Domain — <span className="font-mono text-[var(--accent)]">{domain}</span></CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 gap-3">
          {items.map(item => (
            <div key={item.label} className="text-center p-3 bg-[var(--bg)] rounded-lg border border-[var(--border)]">
              <div className="text-3xl font-light text-[var(--text-primary)]">{item.value}</div>
              <div className="text-xs text-[var(--text-muted)] mt-1 uppercase tracking-wide">{item.label}</div>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  )
}
