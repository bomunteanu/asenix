import { ScatterChart, Scatter, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, Cell } from 'recharts'
import { useEffect, useState } from 'react'
import type { ProcessedAtom } from '#/lib/dashboard-utils'
import { getChartColors } from '#/lib/chart-utils'
import { useTheme } from '#/stores/theme'

interface ValAccuracyScatterChartProps {
  data: ProcessedAtom[]
}

export default function ValAccuracyScatterChart({ data }: ValAccuracyScatterChartProps) {
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

  const [optimizerColors, setOptimizerColors] = useState<Record<string, string>>({
    'sgd': '#2d6a4f',
    'adam': '#6b6860',
    'rmsprop': '#6b6860',
    'unknown': '#6b6860',
  })

  useEffect(() => {
    const chartColors = getChartColors()
    setOptimizerColors({
      'sgd': chartColors.accent,
      'adam': chartColors.textMuted,
      'rmsprop': chartColors.textMuted,
      'unknown': chartColors.textMuted,
    })
  }, [theme])

  const chartData = data
    .filter(atom => atom.val_accuracy !== undefined)
    .map(atom => ({
      x: atom.time_index,
      y: atom.val_accuracy,
      optimizer: atom.optimizer || 'unknown',
      lifecycle: atom.lifecycle,
      atom_id: atom.atom_id,
    }))

  const CustomTooltip = ({ active, payload }: any) => {
    if (active && payload && payload.length) {
      const data = payload[0].payload
      return (
        <div className="bg-[var(--bg)] border border-[var(--border)] p-2 rounded shadow-sm">
          <p className="text-sm font-medium">Atom: {data.atom_id.slice(0, 8)}...</p>
          <p className="text-xs text-[var(--text-muted)]">Accuracy: {(data.y * 100).toFixed(2)}%</p>
          <p className="text-xs text-[var(--text-muted)]">Optimizer: {data.optimizer}</p>
          <p className="text-xs text-[var(--text-muted)]">Lifecycle: {data.lifecycle}</p>
        </div>
      )
    }
    return null
  }

  return (
    <ResponsiveContainer width="100%" height={300}>
      <ScatterChart margin={{ top: 20, right: 20, bottom: 20, left: 20 }}>
        <CartesianGrid strokeDasharray="3 3" stroke={colors.border} />
        <XAxis 
          dataKey="x" 
          name="Time Index" 
          stroke={colors.textMuted}
          label={{ value: 'Time (atom order)', position: 'insideBottom', offset: -10 }}
        />
        <YAxis 
          dataKey="y" 
          name="Val Accuracy" 
          stroke={colors.textMuted}
          label={{ value: 'Validation Accuracy', angle: -90, position: 'insideLeft' }}
          domain={[0.8, 1]}
        />
        <Tooltip content={<CustomTooltip />} />
        <Scatter name="Training Runs" data={chartData}>
          {chartData.map((entry, index) => (
            <Cell 
              key={`cell-${index}`} 
              fill={optimizerColors[entry.optimizer] || optimizerColors.unknown}
            />
          ))}
        </Scatter>
      </ScatterChart>
    </ResponsiveContainer>
  )
}
