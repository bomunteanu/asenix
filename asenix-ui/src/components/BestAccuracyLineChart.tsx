import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, ReferenceLine } from 'recharts'
import { useEffect, useState } from 'react'
import type { BestAccuracyPoint } from '#/lib/dashboard-utils'
import { getChartColors } from '#/lib/chart-utils'
import { useTheme } from '#/stores/theme'

interface BestAccuracyLineChartProps {
  data: BestAccuracyPoint[]
  currentBest: number
}

export default function BestAccuracyLineChart({ data, currentBest }: BestAccuracyLineChartProps) {
  const theme = useTheme((state) => state.theme)
  const [colors, setColors] = useState({
    accent: '#2d6a4f',
    textMuted: '#6b6860',
    border: '#e0ddd8',
  })

  useEffect(() => {
    const chartColors = getChartColors()
    setColors(chartColors)
  }, [theme])

  const CustomTooltip = ({ active, payload }: any) => {
    if (active && payload && payload.length) {
      const data = payload[0].payload
      return (
        <div className="bg-[var(--bg)] border border-[var(--border)] p-2 rounded shadow-sm">
          <p className="text-sm font-medium">Time Index: {data.time_index}</p>
          <p className="text-xs text-[var(--text-muted)]">Best Accuracy: {(data.best_accuracy * 100).toFixed(2)}%</p>
        </div>
      )
    }
    return null
  }

  return (
    <ResponsiveContainer width="100%" height={300}>
      <LineChart data={data} margin={{ top: 20, right: 20, bottom: 20, left: 20 }}>
        <CartesianGrid strokeDasharray="3 3" stroke={colors.border} />
        <XAxis 
          dataKey="time_index" 
          stroke={colors.textMuted}
          label={{ value: 'Time (atom order)', position: 'insideBottom', offset: -10 }}
        />
        <YAxis 
          stroke={colors.textMuted}
          label={{ value: 'Best Validation Accuracy', angle: -90, position: 'insideLeft' }}
          domain={[0.8, 1]}
          tickFormatter={(value) => `${(value * 100).toFixed(1)}%`}
        />
        <Tooltip content={<CustomTooltip />} />
        <ReferenceLine 
          y={currentBest} 
          stroke={colors.accent} 
          strokeDasharray="5 5" 
          label={{ value: `Current Best: ${(currentBest * 100).toFixed(2)}%`, position: 'right' }}
        />
        <Line 
          type="stepAfter" 
          dataKey="best_accuracy" 
          stroke={colors.accent} 
          strokeWidth={2}
          dot={false}
          name="Best Accuracy"
        />
      </LineChart>
    </ResponsiveContainer>
  )
}
