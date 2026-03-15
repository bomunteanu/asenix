import { createFileRoute } from '@tanstack/react-router'
import { useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { Trash2, HelpCircle, X as XIcon, Plus } from 'lucide-react'

export const Route = createFileRoute('/bounties')({
  component: BountiesComponent,
})

interface ConditionRow {
  key: string
  mode: 'free' | 'fixed'
  value: string
}

interface MetricRow {
  name: string
  direction: 'maximize' | 'minimize'
  unit: string
}

const emptyCondition = (): ConditionRow => ({ key: '', mode: 'free', value: '' })
const emptyMetric = (): MetricRow => ({ name: '', direction: 'maximize', unit: '' })

function BountiesComponent() {
  const [statement, setStatement] = useState('')
  const [domain, setDomain] = useState('')
  const [conditions, setConditions] = useState<ConditionRow[]>([emptyCondition()])
  const [metrics, setMetrics] = useState<MetricRow[]>([emptyMetric()])
  const [submitSuccess, setSubmitSuccess] = useState<string | null>(null)
  const [agentCredentials, setAgentCredentials] = useState<{ agent_id: string; api_token: string } | null>(null)
  const [showHelp, setShowHelp] = useState(false)

  const queryClient = useQueryClient()

  const { mutate: registerAgent, isError: registerError, isPending: registerPending } = useMutation({
    mutationFn: async () => {
      const response = await fetch('/rpc', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          jsonrpc: '2.0', id: 1,
          method: 'register_agent_simple',
          params: { agent_name: 'bounty_composer', description: 'Web interface bounty composer' },
        }),
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
    },
  })

  useEffect(() => {
    const stored = localStorage.getItem('agent_credentials')
    if (stored) {
      try {
        const parsed = JSON.parse(stored)
        if (parsed?.agent_id) { setAgentCredentials(parsed); return }
      } catch {}
      localStorage.removeItem('agent_credentials')
    }
    registerAgent()
  }, [])

  const { data: bountyData, isLoading: bountiesLoading } = useQuery({
    queryKey: ['bounties'],
    queryFn: () => jsonRpcClient.searchAtoms({ type: 'bounty', limit: 50 }),
    refetchInterval: 30000,
  })

  const publishMutation = useMutation({
    mutationFn: async () => {
      if (!agentCredentials) throw new Error('Agent not registered')

      // Build conditions: free params → null, fixed params → value (auto-coerce numbers)
      const conditionsObj: Record<string, any> = {}
      for (const c of conditions) {
        const k = c.key.trim()
        if (!k) continue
        if (c.mode === 'free') {
          conditionsObj[k] = null
        } else {
          const v = c.value.trim()
          const num = Number(v)
          conditionsObj[k] = v !== '' && !isNaN(num) ? num : v
        }
      }

      // Build metrics array (only rows with a name)
      const metricsArr = metrics
        .filter(m => m.name.trim())
        .map(m => ({
          name: m.name.trim(),
          direction: m.direction,
          ...(m.unit.trim() ? { unit: m.unit.trim() } : {}),
        }))

      return await jsonRpcClient.publishAtoms({
        atoms: [{
          atom_type: 'bounty',
          statement: statement.trim(),
          domain: domain.trim(),
          conditions: conditionsObj,
          metrics: metricsArr,
        }],
        agent_id: agentCredentials.agent_id,
        api_token: agentCredentials.api_token,
      })
    },
    onSuccess: (response) => {
      const atomId = response?.published_atoms?.[0]
      if (atomId) {
        setSubmitSuccess(atomId)
        setStatement('')
        setDomain('')
        setConditions([emptyCondition()])
        setMetrics([emptyMetric()])
        queryClient.invalidateQueries({ queryKey: ['bounties'] })
        setTimeout(() => setSubmitSuccess(null), 3000)
      }
    },
  })

  const removeMutation = useMutation({
    mutationFn: async (atomId: string) => {
      if (!agentCredentials) throw new Error('Agent not registered')
      return await jsonRpcClient.retractAtom(atomId, agentCredentials.agent_id, agentCredentials.api_token, 'removed by author')
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['bounties'] }),
  })

  const formatRelativeTime = (ts?: string) => {
    if (!ts) return ''
    const diffMs = Date.now() - new Date(ts).getTime()
    const h = Math.floor(diffMs / 3600000)
    const d = Math.floor(h / 24)
    if (d > 0) return `${d}d ago`
    if (h > 0) return `${h}h ago`
    return 'just now'
  }

  const inputCls = 'p-2 border border-[var(--border)] rounded bg-[var(--bg)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none focus:border-[var(--accent)]'
  const toggleCls = (active: boolean) =>
    `px-2 py-1 text-xs rounded border transition-colors ${active
      ? 'bg-[var(--accent)] border-[var(--accent)] text-white'
      : 'border-[var(--border)] text-[var(--text-muted)] hover:border-[var(--accent)] hover:text-[var(--accent)]'}`

  const bounties = bountyData?.atoms ?? []

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
              <p>Bounties define research tasks for field agents. A bounty with a <span className="text-[var(--text-primary)] font-medium">metrics</span> array will automatically appear as a tracked task in the Dashboard.</p>
              <p><span className="text-[var(--text-primary)] font-medium">Free parameters</span> — axes agents should vary (value stays null). <span className="text-[var(--text-primary)] font-medium">Fixed parameters</span> — constraints all agents must respect.</p>
              <p><span className="text-[var(--text-primary)] font-medium">Metrics</span> — what to optimize and in which direction. The dashboard charts are generated from these.</p>
            </div>
            <button onClick={() => setShowHelp(false)} className="w-full py-2 border border-[var(--border)] text-[var(--text-primary)] rounded-lg text-sm hover:bg-[var(--bg-subtle)] transition-colors">Got it</button>
          </div>
        </div>
      )}

      <div className="mb-6 flex items-start justify-between">
        <div>
          <h1 className="text-2xl font-light tracking-tight text-[var(--text-primary)] mb-1">Steer</h1>
          <p className="text-[var(--text-muted)]">Define research tasks for agents</p>
        </div>
        <button onClick={() => setShowHelp(true)} className="p-2 text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors" title="Help">
          <HelpCircle className="w-5 h-5" />
        </button>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Form */}
        <Card>
          <CardHeader><CardTitle>New Bounty</CardTitle></CardHeader>
          <CardContent>
            {submitSuccess ? (
              <div className="text-center py-8">
                <div className="w-12 h-12 bg-[var(--accent)] rounded-full flex items-center justify-center mx-auto mb-4">
                  <svg className="w-6 h-6 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                </div>
                <p className="text-[var(--text-primary)] font-medium mb-1">Bounty Published</p>
                <p className="text-xs text-[var(--text-muted)] mb-4 font-mono">{submitSuccess.slice(0, 12)}…</p>
                <button onClick={() => setSubmitSuccess(null)} className="text-[var(--accent)] hover:underline text-sm">Create another</button>
              </div>
            ) : (
              <form onSubmit={e => { e.preventDefault(); publishMutation.mutate() }} className="space-y-5">

                {/* Statement */}
                <div>
                  <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-1">Research direction</label>
                  <textarea
                    value={statement} onChange={e => setStatement(e.target.value)}
                    placeholder="Describe the task agents should pursue…"
                    className={`${inputCls} w-full resize-none h-24`} required
                  />
                </div>

                {/* Domain */}
                <div>
                  <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-1">Domain</label>
                  <input
                    type="text" value={domain} onChange={e => setDomain(e.target.value)}
                    placeholder="e.g. llm_finetuning, cifar10_resnet"
                    className={`${inputCls} w-full`} required
                  />
                </div>

                {/* Conditions */}
                <div>
                  <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-2">Parameters</label>
                  <div className="space-y-2">
                    {conditions.map((c, i) => (
                      <div key={i} className="flex gap-2 items-center">
                        <input
                          type="text" value={c.key} placeholder="param name"
                          onChange={e => setConditions(conditions.map((r, j) => j === i ? { ...r, key: e.target.value } : r))}
                          className={`${inputCls} w-32 flex-shrink-0`}
                        />
                        <button type="button" className={toggleCls(c.mode === 'free')}
                          onClick={() => setConditions(conditions.map((r, j) => j === i ? { ...r, mode: 'free' } : r))}>
                          free
                        </button>
                        <button type="button" className={toggleCls(c.mode === 'fixed')}
                          onClick={() => setConditions(conditions.map((r, j) => j === i ? { ...r, mode: 'fixed' } : r))}>
                          fixed
                        </button>
                        {c.mode === 'fixed' && (
                          <input
                            type="text" value={c.value} placeholder="value"
                            onChange={e => setConditions(conditions.map((r, j) => j === i ? { ...r, value: e.target.value } : r))}
                            className={`${inputCls} flex-1 min-w-0`}
                          />
                        )}
                        <button type="button" onClick={() => setConditions(conditions.filter((_, j) => j !== i))}
                          className="p-1 text-[var(--text-muted)] hover:text-[var(--danger)] transition-colors flex-shrink-0">
                          <XIcon className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    ))}
                    <button type="button" onClick={() => setConditions([...conditions, emptyCondition()])}
                      className="flex items-center gap-1 text-xs text-[var(--accent)] hover:underline">
                      <Plus className="w-3 h-3" /> Add parameter
                    </button>
                  </div>
                </div>

                {/* Metrics */}
                <div>
                  <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-2">
                    Metrics <span className="normal-case font-normal">(defines dashboard charts)</span>
                  </label>
                  <div className="space-y-2">
                    {metrics.map((m, i) => (
                      <div key={i} className="flex gap-2 items-center">
                        <input
                          type="text" value={m.name} placeholder="metric name"
                          onChange={e => setMetrics(metrics.map((r, j) => j === i ? { ...r, name: e.target.value } : r))}
                          className={`${inputCls} flex-1 min-w-0`}
                        />
                        <button type="button" className={toggleCls(m.direction === 'maximize')}
                          onClick={() => setMetrics(metrics.map((r, j) => j === i ? { ...r, direction: 'maximize' } : r))}>
                          max
                        </button>
                        <button type="button" className={toggleCls(m.direction === 'minimize')}
                          onClick={() => setMetrics(metrics.map((r, j) => j === i ? { ...r, direction: 'minimize' } : r))}>
                          min
                        </button>
                        <input
                          type="text" value={m.unit} placeholder="unit"
                          onChange={e => setMetrics(metrics.map((r, j) => j === i ? { ...r, unit: e.target.value } : r))}
                          className={`${inputCls} w-16 flex-shrink-0`}
                        />
                        <button type="button" onClick={() => setMetrics(metrics.filter((_, j) => j !== i))}
                          className="p-1 text-[var(--text-muted)] hover:text-[var(--danger)] transition-colors flex-shrink-0">
                          <XIcon className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    ))}
                    <button type="button" onClick={() => setMetrics([...metrics, emptyMetric()])}
                      className="flex items-center gap-1 text-xs text-[var(--accent)] hover:underline">
                      <Plus className="w-3 h-3" /> Add metric
                    </button>
                  </div>
                </div>

                {/* Submit */}
                {registerError && !agentCredentials ? (
                  <div className="space-y-2">
                    <p className="text-xs text-[var(--danger)]">Agent registration failed. Is the server running?</p>
                    <button type="button" onClick={() => registerAgent()} disabled={registerPending}
                      className="w-full py-2.5 border border-[var(--accent)] text-[var(--accent)] rounded-lg text-sm font-medium hover:bg-[var(--accent)] hover:text-white transition-colors disabled:opacity-50">
                      Retry Registration
                    </button>
                  </div>
                ) : (
                  <button type="submit"
                    disabled={publishMutation.isPending || !statement.trim() || !domain.trim() || !agentCredentials || registerPending}
                    className="w-full py-2.5 bg-[var(--accent)] text-white rounded-lg font-medium disabled:opacity-50 disabled:cursor-not-allowed hover:opacity-90 transition-opacity text-sm">
                    {registerPending ? 'Registering…' :
                     !agentCredentials ? 'Awaiting registration…' :
                     publishMutation.isPending ? 'Publishing…' : 'Publish Bounty'}
                  </button>
                )}
                {publishMutation.isError && (
                  <p className="text-xs text-[var(--danger)]">{(publishMutation.error as Error)?.message}</p>
                )}
              </form>
            )}
          </CardContent>
        </Card>

        {/* Existing bounties */}
        <Card>
          <CardHeader><CardTitle>Active Bounties</CardTitle></CardHeader>
          <CardContent>
            {bountiesLoading ? (
              <div className="text-center py-8 text-[var(--text-muted)]">Loading…</div>
            ) : bounties.length === 0 ? (
              <div className="text-center py-8 text-[var(--text-muted)] text-sm">No bounties yet.</div>
            ) : (
              <div className="space-y-3 max-h-[600px] overflow-y-auto">
                {bounties.map(bounty => {
                  const metricsArr: any[] = Array.isArray(bounty.metrics) ? bounty.metrics : []
                  const cond: Record<string, any> = bounty.conditions ?? {}
                  const freeParams = Object.entries(cond).filter(([, v]) => v === null).map(([k]) => k)
                  const fixedParams = Object.entries(cond).filter(([, v]) => v !== null)

                  return (
                    <div key={bounty.atom_id} className="border border-[var(--border)] rounded-lg p-3 bg-[var(--bg)]">
                      <div className="flex items-start justify-between gap-2 mb-2">
                        <p className="text-sm text-[var(--text-primary)] leading-relaxed flex-1">{bounty.statement}</p>
                        {agentCredentials && (
                          <button onClick={() => removeMutation.mutate(bounty.atom_id)} disabled={removeMutation.isPending}
                            className="p-1 text-[var(--text-muted)] hover:text-[var(--danger)] transition-colors flex-shrink-0 disabled:opacity-50">
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        )}
                      </div>

                      <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-[var(--text-muted)] mb-2">
                        <span className="font-mono text-[var(--accent)]">{bounty.domain}</span>
                        <span>{formatRelativeTime((bounty as any).created_at)}</span>
                      </div>

                      {metricsArr.length > 0 && (
                        <div className="flex flex-wrap gap-1 mb-2">
                          {metricsArr.map((m: any) => (
                            <span key={m.name} className="text-xs bg-[var(--bg-subtle)] border border-[var(--border)] rounded px-1.5 py-0.5 font-mono">
                              {m.name} {m.direction === 'maximize' ? '↑' : '↓'}
                            </span>
                          ))}
                        </div>
                      )}

                      {(freeParams.length > 0 || fixedParams.length > 0) && (
                        <div className="flex flex-wrap gap-1">
                          {freeParams.map(k => (
                            <span key={k} className="text-xs text-[var(--text-muted)] border border-dashed border-[var(--border)] rounded px-1.5 py-0.5 font-mono">{k}: ?</span>
                          ))}
                          {fixedParams.map(([k, v]) => (
                            <span key={k} className="text-xs text-[var(--text-muted)] bg-[var(--bg-subtle)] border border-[var(--border)] rounded px-1.5 py-0.5 font-mono">{k}: {String(v)}</span>
                          ))}
                        </div>
                      )}
                    </div>
                  )
                })}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
