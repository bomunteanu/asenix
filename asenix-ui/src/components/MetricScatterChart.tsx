import { ScatterChart, Scatter, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, Cell } from 'recharts'
import { useEffect, useState } from 'react'
import type { Run, TaskMetric } from '#/lib/dashboard-utils'
import { getChartColors } from '#/lib/chart-utils'
import { useTheme } from '#/stores/theme'

interface MetricScatterChartProps {
  runs: Run[]
  metric: TaskMetric
}

const LIFECYCLE_COLORS: Record<string, string> = {
  core: 'var(--accent)',
  replicated: 'var(--accent)',
  provisional: 'var(--text-muted)',
  contested: 'var(--danger)',
}

export default function MetricScatterChart({ runs, metric }: MetricScatterChartProps) {
  const theme = useTheme(s => s.theme)
  const [colors, setColors] = useState({ textMuted: '#6b6860', border: '#e0ddd8' })

  useEffect(() => {
    const c = getChartColors()
    setColors({ textMuted: c.textMuted, border: c.border })
  }, [theme])

  const data = runs
    .filter(r => r.metricValues[metric.name] !== undefined)
    .map(r => ({ x: r.time_index, y: r.metricValues[metric.name], lifecycle: r.lifecycle, atom_id: r.atom_id, conditions: r.conditions }))

  const fmt = (v: number) => metric.unit ? `${v} ${metric.unit}` : String(v)

  const CustomTooltip = ({ active, payload }: any) => {
    if (!active || !payload?.length) return null
    const d = payload[0].payload
    return (
      <div className="bg-[var(--bg)] border border-[var(--border)] p-2 rounded shadow-sm text-xs space-y-1">
        <p className="font-medium">{d.atom_id.slice(0, 8)}…</p>
        <p className="text-[var(--text-muted)]">{metric.name}: {fmt(d.y)}</p>
        <p className="text-[var(--text-muted)]">lifecycle: {d.lifecycle}</p>
        {Object.entries(d.conditions).slice(0, 4).map(([k, v]) => (
          <p key={k} className="text-[var(--text-muted)]">{k}: {String(v)}</p>
        ))}
      </div>
    )
  }

  return (
    <ResponsiveContainer width="100%" height={280}>
      <ScatterChart margin={{ top: 16, right: 16, bottom: 24, left: 16 }}>
        <CartesianGrid strokeDasharray="3 3" stroke={colors.border} />
        <XAxis dataKey="x" name="Run order" stroke={colors.textMuted}
          label={{ value: 'run order', position: 'insideBottom', offset: -10, fontSize: 11 }} />
        <YAxis dataKey="y" name={metric.name} stroke={colors.textMuted}
          label={{ value: metric.unit ? `${metric.name} (${metric.unit})` : metric.name, angle: -90, position: 'insideLeft', fontSize: 11 }} />
        <Tooltip content={<CustomTooltip />} />
        <Scatter data={data}>
          {data.map((entry, i) => (
            <Cell key={i} fill={LIFECYCLE_COLORS[entry.lifecycle] ?? colors.textMuted} fillOpacity={0.8} />
          ))}
        </Scatter>
      </ScatterChart>
    </ResponsiveContainer>
  )
}
