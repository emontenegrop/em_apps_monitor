import { NavLink, Outlet } from 'react-router-dom'
import { Activity, Server, Settings } from 'lucide-react'
import { useWebSocket } from '@/hooks/useWebSocket'
import { cn } from '@/lib/utils'

export function Layout() {
  const { connected } = useWebSocket()

  return (
    <div className="flex h-screen overflow-hidden bg-background">
      {/* Sidebar */}
      <aside className="w-56 shrink-0 border-r border-border flex flex-col">
        <div className="px-4 py-5 border-b border-border">
          <div className="flex items-center gap-2">
            <Activity className="h-5 w-5 text-primary" />
            <span className="font-bold tracking-tight">EM Monitor</span>
          </div>
          <div className="flex items-center gap-1.5 mt-2">
            <span className={cn('h-1.5 w-1.5 rounded-full', connected ? 'bg-green-400' : 'bg-red-400')} />
            <span className="text-xs text-muted-foreground">
              {connected ? 'Tiempo real' : 'Reconectando…'}
            </span>
          </div>
        </div>

        <nav className="flex-1 p-2 space-y-0.5">
          <NavLink
            to="/"
            end
            className={({ isActive }) =>
              cn('flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors',
                isActive
                  ? 'bg-primary/20 text-primary'
                  : 'text-muted-foreground hover:bg-muted hover:text-foreground')
            }
          >
            <Activity className="h-4 w-4" />
            Dashboard
          </NavLink>
          <NavLink
            to="/servers"
            className={({ isActive }) =>
              cn('flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors',
                isActive
                  ? 'bg-primary/20 text-primary'
                  : 'text-muted-foreground hover:bg-muted hover:text-foreground')
            }
          >
            <Server className="h-4 w-4" />
            Servidores
          </NavLink>
        </nav>

        <div className="p-4 border-t border-border text-xs text-muted-foreground">
          EM Apps Monitor v0.1
        </div>
      </aside>

      {/* Main content */}
      <main className="flex-1 overflow-auto">
        <Outlet />
      </main>
    </div>
  )
}
