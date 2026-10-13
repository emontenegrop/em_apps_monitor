import { useQuery } from '@tanstack/react-query'
import axios from 'axios'
import { Navigate, Outlet, useLocation } from 'react-router-dom'
import { authApi } from '@/lib/auth'

/** Protege las rutas internas: sin sesión válida redirige a /login */
export function RequireAuth() {
  const location = useLocation()
  const { data: user, isLoading, error } = useQuery({
    queryKey: ['auth', 'me'],
    queryFn: authApi.me,
    retry: false,
    staleTime: 60_000,
  })

  if (isLoading) {
    return <div className="flex h-screen items-center justify-center text-muted-foreground">Cargando…</div>
  }
  if (!user) {
    const unauthorized = axios.isAxiosError(error) && error.response?.status === 401
    if (unauthorized || !error) {
      const next = encodeURIComponent(location.pathname + location.search)
      return <Navigate to={`/login?next=${next}`} replace />
    }
    return (
      <div className="flex h-screen items-center justify-center text-muted-foreground">
        No se pudo contactar al servidor. Reintenta en unos segundos.
      </div>
    )
  }
  return <Outlet />
}
