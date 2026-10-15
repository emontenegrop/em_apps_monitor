import { useEffect, useState } from 'react'
import { NavLink, Outlet, useLocation } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Activity, Globe, HeartPulse, LogOut, Mail, Menu, Server, UserRound, X } from 'lucide-react'
import { useWebSocket } from '@/hooks/useWebSocket'
import { markLoggingOut } from '@/lib/api'
import { authApi } from '@/lib/auth'
import { cn } from '@/lib/utils'

async function logout() {
  markLoggingOut()
  try {
    await authApi.logout()
  } finally {
    // Recarga completa: limpia la caché de consultas y cierra el WebSocket
    window.location.assign('/login')
  }
}

const navClass = ({ isActive }: { isActive: boolean }) =>
  cn('flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors',
    isActive
      ? 'bg-primary/20 text-primary'
      : 'text-muted-foreground hover:bg-muted hover:text-foreground')

function ConnectionStatus({ connected }: { connected: boolean }) {
  return (
    <div className="flex items-center gap-1.5">
      <span className={cn('h-1.5 w-1.5 rounded-full', connected ? 'bg-green-400' : 'bg-red-400')} />
      <span className="text-xs text-muted-foreground">
        {connected ? 'Tiempo real' : 'Reconectando…'}
      </span>
    </div>
  )
}

export function Layout() {
  const { connected } = useWebSocket()
  const { data: user } = useQuery({ queryKey: ['auth', 'me'], queryFn: authApi.me })
  // Menú lateral en pantallas chicas
  const [menuOpen, setMenuOpen] = useState(false)
  const location = useLocation()

  useEffect(() => setMenuOpen(false), [location.pathname])

  return (
    <div className="flex h-dvh overflow-hidden bg-background">
      {/* Fondo del menú en pantallas chicas */}
      {menuOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/60 md:hidden"
          onClick={() => setMenuOpen(false)}
          aria-hidden
        />
      )}

      {/* Sidebar: fija desde md, deslizable en celulares */}
      <aside
        className={cn(
          'fixed inset-y-0 left-0 z-50 w-64 bg-background border-r border-border flex flex-col transition-transform',
          'md:static md:z-auto md:w-56 md:shrink-0 md:translate-x-0',
          menuOpen ? 'translate-x-0' : '-translate-x-full',
        )}
      >
        <div className="px-4 py-5 border-b border-border">
          <div className="flex items-center gap-2">
            <Activity className="h-5 w-5 text-primary" />
            <span className="font-bold tracking-tight">EM Monitor</span>
            <button
              type="button"
              className="ml-auto p-1 rounded-md text-muted-foreground hover:bg-muted md:hidden"
              onClick={() => setMenuOpen(false)}
              aria-label="Cerrar menú"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
          <div className="mt-2">
            <ConnectionStatus connected={connected} />
          </div>
        </div>

        <nav className="flex-1 p-2 space-y-0.5">
          <NavLink to="/" end className={navClass}>
            <Activity className="h-4 w-4" />
            Dashboard
          </NavLink>
          <NavLink to="/servers" className={navClass}>
            <Server className="h-4 w-4" />
            Servidores
          </NavLink>

          <div className="px-3 pt-4 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            Disponibilidad
          </div>
          <NavLink to="/uptime" end className={navClass}>
            <HeartPulse className="h-4 w-4" />
            Disponibilidad
          </NavLink>
          <NavLink to="/uptime/targets" className={navClass}>
            <Globe className="h-4 w-4" />
            URLs monitoreadas
          </NavLink>
          <NavLink to="/settings/notifications" className={navClass}>
            <Mail className="h-4 w-4" />
            Notificaciones
          </NavLink>
        </nav>

        <div className="p-2 border-t border-border space-y-0.5">
          <NavLink to="/account" className={navClass}>
            <UserRound className="h-4 w-4" />
            <span className="truncate">{user?.username ?? 'Mi cuenta'}</span>
          </NavLink>
          <button
            type="button"
            onClick={logout}
            className="w-full flex items-center gap-2 px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <LogOut className="h-4 w-4" />
            Salir
          </button>
          <div className="px-3 pt-2 text-xs text-muted-foreground">EM Apps Monitor v0.1</div>
        </div>
      </aside>

      <div className="flex-1 min-w-0 flex flex-col">
        {/* Barra superior en celulares */}
        <header className="md:hidden flex items-center gap-3 px-4 h-12 shrink-0 border-b border-border">
          <button
            type="button"
            className="p-1.5 -ml-1.5 rounded-md text-muted-foreground hover:bg-muted"
            onClick={() => setMenuOpen(true)}
            aria-label="Abrir menú"
          >
            <Menu className="h-5 w-5" />
          </button>
          <Activity className="h-4 w-4 text-primary" />
          <span className="font-bold tracking-tight text-sm">EM Monitor</span>
          <div className="ml-auto">
            <ConnectionStatus connected={connected} />
          </div>
        </header>

        {/* Main content */}
        <main className="flex-1 overflow-auto">
          <Outlet />
        </main>
      </div>
    </div>
  )
}
