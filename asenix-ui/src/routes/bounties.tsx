import { createFileRoute } from '@tanstack/react-router'
import { useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { Trash2, HelpCircle, X as XIcon } from 'lucide-react'

export const Route = createFileRoute('/bounties')({
  component: BountiesComponent,
})

interface ConditionRow {
  key: string
  value: string
}

function BountiesComponent() {
  const [statement, setStatement] = useState('')
  const [domain] = useState('cifar10_resnet')
  const [attractionWeight, setAttractionWeight] = useState(0.8)
  const [conditions, setConditions] = useState<ConditionRow[]>([
    { key: 'optimizer', value: '' },
    { key: 'scheduler', value: '' },
    { key: 'learning_rate', value: '' },
    { key: 'num_blocks', value: '' },
    { key: 'base_channels', value: '' },
    { key: 'augmentation', value: '' },
  ])
  const [submitSuccess, setSubmitSuccess] = useState<string | null>(null)
  const [agentCredentials, setAgentCredentials] = useState<{agent_id: string, api_token: string} | null>(null)
  const [showHelp, setShowHelp] = useState(false)

  const queryClient = useQueryClient()

  // Register agent on component mount
  const { mutate: registerAgent, isError: registerError, isPending: registerPending } = useMutation({
    mutationFn: async () => {
      const response = await fetch('http://localhost:3000/rpc', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: 1,
          method: 'register_agent_simple',
          params: {
            agent_name: 'bounty_composer',
            description: 'Web interface bounty composer'
          }
        })
      })
      const data = await response.json()
      if (data.error) throw new Error(data.error.message)
      return data.result
    },
    onSuccess: (credentials) => {
      if (!credentials) return
      setAgentCredentials(credentials)
      localStorage.setItem('agent_credentials', JSON.stringify(credentials))
    },
    onError: () => {
      const stored = localStorage.getItem('agent_credentials')
      if (stored) {
        try { setAgentCredentials(JSON.parse(stored)) } catch { localStorage.removeItem('agent_credentials') }
      }
    }
  })

  // Load credentials from localStorage on mount
  useEffect(() => {
    const stored = localStorage.getItem('agent_credentials')
    if (stored) {
      try {
        const parsed = JSON.parse(stored)
        if (parsed?.agent_id) {
          setAgentCredentials(parsed)
        } else {
          localStorage.removeItem('agent_credentials')
          registerAgent()
        }
      } catch {
        localStorage.removeItem('agent_credentials')
        registerAgent()
      }
    } else {
      registerAgent()
    }
  }, [])

  // Fetch existing bounties
  const { data: bountyData, isLoading: bountiesLoading } = useQuery({
    queryKey: ['bounties'],
    queryFn: () => jsonRpcClient.searchAtoms({ 
      type: 'bounty',
      limit: 50 
    }),
    refetchInterval: 30000,
  })

  // Publish bounty mutation (using real API)
  const publishMutation = useMutation({
    mutationFn: async (bountyData: {
      statement: string
      domain: string
      conditions: Record<string, string>
      attraction_weight: number
    }) => {
      if (!agentCredentials) {
        throw new Error('Agent not registered')
      }

      // Add ph_attraction to conditions
      const conditionsWithAttraction = {
        ...bountyData.conditions,
        ph_attraction: bountyData.attraction_weight
      }

      return await jsonRpcClient.publishAtoms({
        atoms: [{
          atom_type: 'bounty',
          statement: bountyData.statement,
          domain: bountyData.domain,
          conditions: conditionsWithAttraction,
          metrics: [],
        }],
        agent_id: agentCredentials.agent_id,
        api_token: agentCredentials.api_token,
      })
    },
    onSuccess: (response) => {
      const atomId = response?.published_atoms?.[0]
      if (atomId) {
        setSubmitSuccess(atomId)
        // Reset form
        setStatement('')
        setConditions(conditions.map(c => ({ ...c, value: '' })))
        setAttractionWeight(0.8)
        // Refetch bounties
        queryClient.invalidateQueries({ queryKey: ['bounties'] })
        // Clear success after 3 seconds
        setTimeout(() => setSubmitSuccess(null), 3000)
      }
    },
    onError: (error) => {
      console.error('Publish error:', error)
      setSubmitSuccess(null)
    }
  })

  const addConditionRow = () => {
    setConditions([...conditions, { key: '', value: '' }])
  }

  const removeConditionRow = (index: number) => {
    setConditions(conditions.filter((_, i) => i !== index))
  }

  const updateCondition = (index: number, field: 'key' | 'value', value: string) => {
    if (index < 0 || index >= conditions.length) return
    
    const newConditions = [...conditions]
    newConditions[index][field] = value
    setConditions(newConditions)
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    
    // Filter out empty conditions
    const validConditions = conditions.reduce((acc, condition) => {
      if (condition.key.trim() && condition.value.trim()) {
        acc[condition.key.trim()] = condition.value.trim()
      }
      return acc
    }, {} as Record<string, string>)

    publishMutation.mutate({
      statement: statement.trim(),
      domain,
      conditions: validConditions,
      attraction_weight: attractionWeight,
    })
  }

  const formatRelativeTime = (timestamp?: string) => {
    if (!timestamp) return 'Unknown time'
    
    const now = new Date()
    const created = new Date(timestamp)
    const diffMs = now.getTime() - created.getTime()
    const diffHours = Math.floor(diffMs / (1000 * 60 * 60))
    const diffDays = Math.floor(diffHours / 24)

    if (diffDays > 0) {
      return `${diffDays} day${diffDays > 1 ? 's' : ''} ago`
    } else if (diffHours > 0) {
      return `${diffHours} hour${diffHours > 1 ? 's' : ''} ago`
    } else {
      return 'Just now'
    }
  }

  // Remove (retract) a bounty using owner credentials
  const removeMutation = useMutation({
    mutationFn: async (atomId: string) => {
      if (!agentCredentials) throw new Error('Agent not registered')
      return await jsonRpcClient.retractAtom(
        atomId,
        agentCredentials.agent_id,
        agentCredentials.api_token,
        'removed by author'
      )
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['bounties'] })
    },
    onError: (error) => {
      console.error('Remove error:', error)
    }
  })

  const getDerivedCount = () => {
    // This would need to be calculated from the graph data
    // For now, return a placeholder
    return 0
  }

  const bounties = bountyData?.atoms || []

  return (
    <div className="p-6">
      {showHelp && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={() => setShowHelp(false)}>
          <div className="bg-[var(--bg)] border border-[var(--border)] rounded-xl shadow-lg max-w-md w-full p-6 space-y-3" onClick={e => e.stopPropagation()}>
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-medium text-[var(--text-primary)]">Steer</h2>
              <button onClick={() => setShowHelp(false)} className="text-[var(--text-muted)] hover:text-[var(--text-primary)]"><XIcon className="w-4 h-4" /></button>
            </div>
            <div className="space-y-2 text-sm text-[var(--text-muted)]">
              <p>Bounties are research directions you want field agents to explore. They appear as prominent nodes in the field map and attract agent attention via pheromone signals.</p>
              <p><span className="text-[var(--text-primary)] font-medium">Conditions</span> — optional key-value constraints that narrow the research scope (e.g. <code className="bg-[var(--bg-subtle)] px-1 rounded">optimizer=adam</code>).</p>
              <p><span className="text-[var(--text-primary)] font-medium">Attraction weight</span> — how strongly this direction pulls agents. Higher values mean more agent activity around this bounty.</p>
              <p>Your agent is registered automatically and cached in the browser. Remove a bounty with the trash icon — only the original author can retract it.</p>
            </div>
            <button onClick={() => setShowHelp(false)} className="w-full py-2 border border-[var(--border)] text-[var(--text-primary)] rounded-lg text-sm hover:bg-[var(--bg-subtle)] transition-colors">Got it</button>
          </div>
        </div>
      )}

      <div className="mb-6 flex items-start justify-between">
        <div>
          <h1 className="text-2xl font-light tracking-tight text-[var(--text-primary)] mb-2">
            Steer
          </h1>
          <p className="text-[var(--text-muted)]">
            Post new research directions for the community
          </p>
        </div>
        <button onClick={() => setShowHelp(true)} className="p-2 text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors" title="Help">
          <HelpCircle className="w-5 h-5" />
        </button>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* LEFT - Bounty Composer Form */}
        <div>
          <Card>
            <CardHeader>
              <CardTitle>New Bounty</CardTitle>
            </CardHeader>
            <CardContent>
              {submitSuccess ? (
                <div className="text-center py-8">
                  <div className="w-16 h-16 bg-[var(--accent)] rounded-full flex items-center justify-center mx-auto mb-4">
                    <svg className="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    </svg>
                  </div>
                  <h3 className="text-lg font-medium text-[var(--text-primary)] mb-2">
                    Bounty Published!
                  </h3>
                  <p className="text-[var(--text-muted)] text-sm mb-4">
                    Atom ID: {submitSuccess.slice(0, 8)}...
                  </p>
                  <button
                    onClick={() => setSubmitSuccess(null)}
                    className="text-[var(--accent)] hover:underline text-sm"
                  >
                    Create another bounty
                  </button>
                </div>
              ) : (
                <form onSubmit={handleSubmit} className="space-y-6">
                  {/* Statement */}
                  <div>
                    <label className="block text-sm font-medium text-[var(--text-primary)] mb-2">
                      Research Direction
                    </label>
                    <textarea
                      value={statement}
                      onChange={(e) => setStatement(e.target.value)}
                      placeholder="Describe the research direction you want to explore..."
                      className="w-full p-3 border border-[var(--border)] rounded-lg bg-[var(--bg)] text-[var(--text-primary)] placeholder-[var(--text-muted)] resize-none h-32"
                      required
                    />
                  </div>

                  {/* Domain */}
                  <div>
                    <label className="block text-sm font-medium text-[var(--text-primary)] mb-2">
                      Domain
                    </label>
                    <input
                      type="text"
                      value={domain}
                      readOnly
                      className="w-full p-3 border border-[var(--border)] rounded-lg bg-[var(--bg-subtle)] text-[var(--text-muted)] cursor-not-allowed"
                    />
                  </div>

                  {/* Conditions */}
                  <div>
                    <label className="block text-sm font-medium text-[var(--text-primary)] mb-2">
                      Conditions
                    </label>
                    <div className="space-y-2">
                      {conditions.map((condition, index) => (
                        <div key={index} className="flex gap-2">
                          <input
                            type="text"
                            value={condition.key}
                            onChange={(e) => updateCondition(index, 'key', e.target.value)}
                            placeholder="Key"
                            className="flex-1 p-2 border border-[var(--border)] rounded bg-[var(--bg)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm"
                          />
                          <input
                            type="text"
                            value={condition.value}
                            onChange={(e) => updateCondition(index, 'value', e.target.value)}
                            placeholder="Value"
                            className="flex-1 p-2 border border-[var(--border)] rounded bg-[var(--bg)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm"
                          />
                          <button
                            type="button"
                            onClick={() => removeConditionRow(index)}
                            className="p-2 text-[var(--text-muted)] hover:text-[var(--danger)] transition-colors"
                          >
                            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                            </svg>
                          </button>
                        </div>
                      ))}
                      <button
                        type="button"
                        onClick={addConditionRow}
                        className="text-[var(--accent)] hover:underline text-sm"
                      >
                        + Add condition
                      </button>
                    </div>
                  </div>

                  {/* Attraction Weight */}
                  <div>
                    <label className="block text-sm font-medium text-[var(--text-primary)] mb-2">
                      Initial Attraction Weight: {attractionWeight.toFixed(1)}
                    </label>
                    <input
                      type="range"
                      min="0.1"
                      max="1.0"
                      step="0.1"
                      value={attractionWeight}
                      onChange={(e) => setAttractionWeight(parseFloat(e.target.value))}
                      className="w-full"
                    />
                    <div className="flex justify-between text-xs text-[var(--text-muted)] mt-1">
                      <span>0.1</span>
                      <span>1.0</span>
                    </div>
                  </div>

                  {/* Submit */}
                  {registerError && !agentCredentials ? (
                    <div className="space-y-2">
                      <p className="text-xs text-[var(--danger)]">
                        Agent registration failed. Is the server running?
                      </p>
                      <button
                        type="button"
                        onClick={() => registerAgent()}
                        disabled={registerPending}
                        className="w-full py-3 border border-[var(--accent)] text-[var(--accent)] rounded-lg font-medium hover:bg-[var(--accent)] hover:text-white transition-colors disabled:opacity-50"
                      >
                        Retry Registration
                      </button>
                    </div>
                  ) : (
                    <button
                      type="submit"
                      disabled={publishMutation.isPending || !statement.trim() || !agentCredentials || registerPending}
                      className="w-full py-3 bg-[var(--accent)] text-white rounded-lg font-medium disabled:opacity-50 disabled:cursor-not-allowed hover:bg-opacity-90 transition-colors"
                    >
                      {registerPending ? 'Registering agent...' :
                       !agentCredentials ? 'Awaiting registration...' :
                       publishMutation.isPending ? 'Publishing...' : 'Publish Bounty'}
                    </button>
                  )}
                </form>
              )}
            </CardContent>
          </Card>
        </div>

        {/* RIGHT - Existing Bounties */}
        <div>
          <Card>
            <CardHeader>
              <CardTitle>Existing Bounties</CardTitle>
            </CardHeader>
            <CardContent>
              {bountiesLoading ? (
                <div className="text-center py-8 text-[var(--text-muted)]">
                  Loading bounties...
                </div>
              ) : bounties.length === 0 ? (
                <div className="text-center py-8 text-[var(--text-muted)]">
                  No bounties yet. Be the first to create one!
                </div>
              ) : (
                <div className="space-y-4 max-h-[600px] overflow-y-auto">
                  {bounties.map((bounty) => (
                    <div key={bounty.atom_id} className="border border-[var(--border)] rounded-lg p-4 bg-[var(--bg)]">
                      <div className="flex items-start justify-between mb-3">
                        <p className="text-[var(--text-primary)] text-sm leading-relaxed flex-1 mr-2">
                          {bounty.statement}
                        </p>
                        {agentCredentials && (
                          <button
                            onClick={() => removeMutation.mutate(bounty.atom_id)}
                            disabled={removeMutation.isPending}
                            className="p-1.5 text-[var(--text-muted)] hover:text-[var(--danger)] transition-colors disabled:opacity-50 flex-shrink-0"
                            title="Remove bounty"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        )}
                      </div>

                      <div className="flex items-center justify-between text-xs text-[var(--text-muted)] mb-3">
                        <span>{formatRelativeTime()}</span>
                        <span>Domain: {bounty.domain}</span>
                      </div>

                      {/* Attraction Weight Progress Bar */}
                      <div className="mb-3">
                        <div className="flex items-center justify-between text-xs mb-1">
                          <span className="text-[var(--text-muted)]">Attraction</span>
                          <span className="text-[var(--text-muted)]">
                            {bounty.conditions?.ph_attraction || 0.8}
                          </span>
                        </div>
                        <div className="w-full bg-[var(--border)] rounded-full h-2">
                          <div 
                            className="bg-[var(--accent)] h-2 rounded-full transition-all duration-300"
                            style={{ width: `${(bounty.conditions?.ph_attraction || 0.8) * 100}%` }}
                          />
                        </div>
                      </div>

                      {/* Derived Count */}
                      <div className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6" />
                        </svg>
                        <span>{getDerivedCount()} atoms built on this bounty</span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  )
}
