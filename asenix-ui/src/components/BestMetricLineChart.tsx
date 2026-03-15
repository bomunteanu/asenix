import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, ReferenceLine } from 'recharts'
import { useEffect, useState } from 'react'
import type { BestPoint, TaskMetric } from '#/lib/dashboard-utils'
import { getChartColors } from '#/lib/chart-utils'
import { useTheme } from '#/stores/theme'

interface BestMetricLineChartProps {
  data: BestPoint[]
  metric: TaskMetric
}

export default function BestMetricLineChart({ data, metric }: BestMetricLineChartProps) {
  const theme = useTheme(s => s.theme)
  const [colors, setColors] = useState({ accent: '#2d6a4f', textMuted: '#6b6860', border: '#e0ddd8' })

  useEffect(() => {
    const c = getChartColors()
    setColors({ accent: c.accent, textMuted: c.textMuted, border: c.border })
  }, [theme])

  const currentBest = data.length > 0 ? data[data.length - 1].best : null
  const fmt = (v: number) => metric.unit ? `${v} ${metric.unit}` : String(v)

  const CustomTooltip = ({ active, payload }: any) => {
    if (!active || !payload?.length) return null
    const d = payload[0].payload
    return (
      <div className="bg-[var(--bg)] border border-[var(--border)] p-2 rounded shadow-sm text-xs">
        <p className="text-[var(--text-muted)]">run {d.time_index}</p>
        <p className="font-medium">best {metric.name}: {fmt(d.best)}</p>
      </div>
    )
  }

  const allValues = data.map(d => d.best)
  const minVal = allValues.length ? Math.min(...allValues) : 0
  const maxVal = allValues.length ? Math.max(...allValues) : 1
  const pad = (maxVal - minVal) * 0.1 || 0.1
  const domainMin = metric.direction === 'maximize' ? minVal - pad : minVal - pad
  const domainMax = maxVal + pad

  return (
    <ResponsiveContainer width="100%" height={280}>
      <LineChart data={data} margin={{ top: 16, right: 48, bottom: 24, left: 16 }}>
        <CartesianGrid strokeDasharray="3 3" stroke={colors.border} />
        <XAxis dataKey="time_index" stroke={colors.textMuted}
          label={{ value: 'run order', position: 'insideBottom', offset: -10, fontSize: 11 }} />
        <YAxis stroke={colors.textMuted} domain={[domainMin, domainMax]}
          label={{ value: metric.unit ? `${metric.name} (${metric.unit})` : metric.name, angle: -90, position: 'insideLeft', fontSize: 11 }}
          tickFormatter={v => String(+v.toFixed(4))} />
        <Tooltip content={<CustomTooltip />} />
        {currentBest !== null && (
          <ReferenceLine y={currentBest} stroke={colors.accent} strokeDasharray="5 5"
            label={{ value: fmt(+currentBest.toFixed(4)), position: 'right', fontSize: 11, fill: colors.accent }} />
        )}
        <Line type="stepAfter" dataKey="best" stroke={colors.accent} strokeWidth={2} dot={false} />
      </LineChart>
    </ResponsiveContainer>
  )
}
