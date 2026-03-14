import { Outlet } from '@tanstack/react-router'
import { Link } from '@tanstack/react-router'
import { 
  Network, 
  BarChart, 
  Target, 
  Inbox,
  Sun,
  Moon
} from 'lucide-react'
import { useQuery } from '@tanstack/react-query'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { useTheme } from '#/stores/theme'
import type { ReactNode } from 'react'

interface LayoutProps {
  children?: ReactNode
}

export default function Layout({ children }: LayoutProps) {
  const { data: atomsData } = useQuery({
    queryKey: ['atomCount'],
    queryFn: () => jsonRpcClient.searchAtoms({ limit: 1 }),
    refetchInterval: 30000, // Refresh every 30 seconds
  })

  const { theme, toggleTheme } = useTheme()

  const atomCount = atomsData?.atoms?.length || 0

  const navigationItems = [
    { to: '/', label: 'Map', icon: Network },
    { to: '/dashboard', label: 'Dashboard', icon: BarChart },
    { to: '/bounties', label: 'Steer', icon: Target },
    { to: '/queue', label: 'Review Queue', icon: Inbox },
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
              className="w-10 h-10"
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
            <h2 className="text-lg font-light tracking-tight">
              cifar10_resnet
            </h2>
            <span className="text-sm text-[var(--text-muted)]">
              {atomCount} atoms
            </span>
          </div>
          
          <div className="flex items-center gap-4">
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
            
            {/* Placeholder for future actions */}
            <div className="w-8 h-8 rounded-full bg-[var(--bg-subtle)] flex items-center justify-center">
              <div className="w-4 h-4 rounded-full bg-[var(--accent)]"></div>
            </div>
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
