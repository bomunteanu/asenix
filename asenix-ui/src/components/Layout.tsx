import { Outlet } from '@tanstack/react-router'
import { Link } from '@tanstack/react-router'
import {
  Network,
  BarChart,
  Target,
  Inbox,
  Sun,
  Moon,
  ShieldCheck,
  FolderOpen,
} from 'lucide-react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { useTheme } from '#/stores/theme'
import { useLiveFeed } from '#/stores/live-feed'
import { useActiveProject } from '#/stores/active-project'
import ProjectSwitcher from '#/components/ProjectSwitcher'
import { useSSE } from '#/lib/use-sse'
import { useEffect } from 'react'
import type { ReactNode } from 'react'

interface LayoutProps {
  children?: ReactNode
}

export default function Layout({ children }: LayoutProps) {
  const { activeProject } = useActiveProject()
  const { data: atomsData } = useQuery({
    queryKey: ['atomCount', activeProject?.project_id],
    queryFn: () => jsonRpcClient.searchAtoms({
      limit: 1,
      project_id: activeProject?.project_id,
    }),
    refetchInterval: 30000, // Refresh every 30 seconds
  })

  const { theme, toggleTheme } = useTheme()
  const queryClient = useQueryClient()
  const setRecentEvents = useLiveFeed(s => s.setRecentEvents)
  const setRecentAtomIds = useLiveFeed(s => s.setRecentAtomIds)
  const recentEvents = useLiveFeed(s => s.recentEvents)

  const { recentEvents: sseEvents, recentAtomIds } = useSSE({
    types: ['atom_published'],
    onEvent: () => queryClient.invalidateQueries({ queryKey: ['atomCount'] }),
  })

  useEffect(() => { setRecentEvents(sseEvents) }, [sseEvents, setRecentEvents])
  useEffect(() => { setRecentAtomIds(recentAtomIds) }, [recentAtomIds, setRecentAtomIds])

  const atomCount = atomsData?.total ?? 0

  const navigationItems = [
    { to: '/', label: 'Map', icon: Network },
    { to: '/dashboard', label: 'Dashboard', icon: BarChart },
    { to: '/bounties', label: 'Steer', icon: Target },
    { to: '/queue', label: 'Review Queue', icon: Inbox },
    { to: '/projects', label: 'Projects', icon: FolderOpen },
    { to: '/admin', label: 'Admin', icon: ShieldCheck },
  ]

  return (
    <div className="flex h-screen bg-[var(--bg)] text-[var(--text-primary)]">
      {/* Left Sidebar */}
      <aside className="w-64 bg-[var(--bg-subtle)] flex flex-col">
        <div className="p-4">
          <div className="flex items-center gap-3">
            <img 
              src={theme === 'dark' ? '/logo_dark_mode.png' : '/logo_light_mode.png'}
              alt="Asenix Logo"
              className="w-8 h-8"
            />
            <h1 className="text-xl font-light tracking-tight">Asenix</h1>
          </div>
        </div>
        
        <nav className="flex-1 p-4">
          <ul className="space-y-1">
            {navigationItems.map(({ to, label, icon: Icon }) => (
              <li key={to}>
                <Link
                  to={to}
                  className="flex items-center gap-3 px-3 py-2 rounded text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors [&.is-active]:text-[var(--text-primary)] [&.is-active]:font-medium"
                  activeProps={{ className: 'is-active' }}
                >
                  <Icon className="w-4 h-4" />
                  <span>{label}</span>
                </Link>
              </li>
            ))}
          </ul>
        </nav>
      </aside>

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col">
        {/* Top Bar */}
        <header className="h-16 bg-[var(--bg)] border-b border-[var(--border)] flex items-center justify-between px-6">
          <div className="flex items-center gap-4">
            <span className="text-sm text-[var(--text-muted)]">
              {atomCount} atoms
            </span>
            {recentEvents.length > 0 && (
              <div className="flex items-center gap-2 overflow-hidden max-w-xs">
                {recentEvents.map(ev => (
                  <span
                    key={ev.id}
                    className="text-xs text-[var(--text-muted)] whitespace-nowrap animate-fade-in"
                    title={ev.atom_id}
                  >
                    • {ev.atom_type ?? ev.event_type}{ev.domain ? ` in ${ev.domain}` : ''}
                  </span>
                ))}
              </div>
            )}
          </div>
          
          <div className="flex items-center gap-3">
            <ProjectSwitcher />

            {/* Theme Toggle */}
            <button
              onClick={toggleTheme}
              className="w-8 h-8 rounded-full bg-[var(--bg-subtle)] flex items-center justify-center hover:bg-[var(--border)] transition-colors"
              aria-label="Toggle theme"
            >
              {theme === 'light' ? (
                <Moon className="w-4 h-4 text-[var(--text-muted)]" />
              ) : (
                <Sun className="w-4 h-4 text-[var(--text-muted)]" />
              )}
            </button>
          </div>
        </header>

        {/* Main Content */}
        <main className="flex-1 overflow-auto p-6">
          {children || <Outlet />}
        </main>
      </div>
    </div>
  )
}
