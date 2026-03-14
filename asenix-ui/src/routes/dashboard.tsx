import { createFileRoute } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import ValAccuracyScatterChart from '#/components/ValAccuracyScatterChart'
import BestAccuracyLineChart from '#/components/BestAccuracyLineChart'
import StatsPanel from '#/components/StatsPanel'
import TopRunsTable from '#/components/TopRunsTable'
import { 
  processAtoms, 
  calculateStats, 
  getTopRuns, 
  calculateBestAccuracyOverTime 
} from '#/lib/dashboard-utils'

export const Route = createFileRoute('/dashboard')({
  component: DashboardComponent,
})

function DashboardComponent() {
  const { data: atomsData, isLoading, error } = useQuery({
    queryKey: ['dashboardAtoms'],
    queryFn: () => jsonRpcClient.searchAtoms({ 
      domain: 'cifar10_resnet',
      limit: 1000 // Get all atoms for this domain
    }),
    refetchInterval: 30000, // Refresh every 30 seconds
  })

  if (isLoading) {
    return (
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {[...Array(4)].map((_, i) => (
          <Card key={i} className="h-80">
            <CardContent className="flex items-center justify-center h-full">
              <div className="text-[var(--sea-ink-soft)]">Loading...</div>
            </CardContent>
          </Card>
        ))}
      </div>
    )
  }

  if (error) {
    return (
      <div className="text-center py-8">
        <div className="text-red-400">Error loading dashboard data</div>
        <div className="text-sm text-[var(--sea-ink-soft)] mt-2">
          {error instanceof Error ? error.message : 'Unknown error'}
        </div>
      </div>
    )
  }

  const atoms = atomsData?.atoms || []
  const processedAtoms = processAtoms(atoms)
  const stats = calculateStats(atoms)
  const topRuns = getTopRuns(processedAtoms)
  const bestAccuracyData = calculateBestAccuracyOverTime(processedAtoms)
  const currentBest = bestAccuracyData.length > 0 
    ? bestAccuracyData[bestAccuracyData.length - 1].best_accuracy 
    : 0

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold text-[var(--sea-ink)]">Dashboard</h1>
      
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Val Accuracy Scatter Chart */}
        <Card>
          <CardHeader>
            <CardTitle>Validation Accuracy Over Time</CardTitle>
          </CardHeader>
          <CardContent>
            <ValAccuracyScatterChart data={processedAtoms} />
          </CardContent>
        </Card>

        {/* Best Val Accuracy Line Chart */}
        <Card>
          <CardHeader>
            <CardTitle>Best Validation Accuracy Over Time</CardTitle>
          </CardHeader>
          <CardContent>
            <BestAccuracyLineChart 
              data={bestAccuracyData} 
              currentBest={currentBest}
            />
          </CardContent>
        </Card>

        {/* Stats Panel */}
        <StatsPanel stats={stats} />

        {/* Top Runs Table */}
        <TopRunsTable runs={topRuns} />
      </div>
    </div>
  )
}
