import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useAdminAuth } from '#/stores/admin-auth'
import { Lock, LogOut, ShieldCheck } from 'lucide-react'

export const Route = createFileRoute('/admin')({
  component: AdminPage,
})

const API_BASE = import.meta.env.VITE_API_URL ?? 'http://localhost:3000'

function AdminPage() {
  const { token, setToken, logout } = useAdminAuth()
  const [secret, setSecret] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)
    setLoading(true)
    try {
      const res = await fetch(`${API_BASE}/admin/login`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ secret }),
      })
      if (!res.ok) {
        const body = await res.json().catch(() => ({}))
        throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`)
      }
      const data = await res.json() as { token: string }
      setToken(data.token)
      setSecret('')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Login failed')
    } finally {
      setLoading(false)
    }
  }

  if (token) {
    return (
      <div className="max-w-md mx-auto mt-20 p-8 bg-[var(--bg-subtle)] border border-[var(--border)] rounded-xl space-y-4">
        <div className="flex items-center gap-3 text-[var(--accent)]">
          <ShieldCheck className="w-6 h-6" />
          <h2 className="text-lg font-medium">Admin authenticated</h2>
        </div>
        <p className="text-sm text-[var(--text-muted)]">
          Your admin session is active. The JWT is stored locally and included in review/admin requests.
        </p>
        <button
          onClick={logout}
          className="flex items-center gap-2 text-sm text-[var(--danger)] hover:underline"
        >
          <LogOut className="w-4 h-4" />
          Log out
        </button>
      </div>
    )
  }

  return (
    <div className="max-w-md mx-auto mt-20 p-8 bg-[var(--bg-subtle)] border border-[var(--border)] rounded-xl space-y-6">
      <div className="flex items-center gap-3">
        <Lock className="w-5 h-5 text-[var(--text-muted)]" />
        <h2 className="text-lg font-medium">Admin login</h2>
      </div>
      <form onSubmit={handleLogin} className="space-y-4">
        <div>
          <label className="block text-sm text-[var(--text-muted)] mb-1">
            Owner password
          </label>
          <input
            type="password"
            value={secret}
            onChange={e => setSecret(e.target.value)}
            placeholder="OWNER_SECRET"
            required
            className="w-full px-3 py-2 bg-[var(--bg)] border border-[var(--border)] rounded-lg text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
          />
        </div>
        {error && (
          <p className="text-sm text-[var(--danger)]">{error}</p>
        )}
        <button
          type="submit"
          disabled={loading || !secret}
          className="w-full py-2 bg-[var(--accent)] text-white rounded-lg text-sm font-medium hover:opacity-90 disabled:opacity-50 transition-opacity"
        >
          {loading ? 'Authenticating…' : 'Log in'}
        </button>
      </form>
    </div>
  )
}
