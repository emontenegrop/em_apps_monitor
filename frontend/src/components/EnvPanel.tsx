import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Eye, EyeOff, KeyRound } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { containersApi } from '@/lib/api'
import { apiErrorMessage } from '@/lib/auth'

/** Variables de entorno del contenedor. Los valores llegan ocultos (`***`) salvo que se pidan. */
export function EnvPanel({ serverId, containerId }: { serverId: string; containerId: string }) {
  const [reveal, setReveal] = useState(false)
  const { data, isLoading, error } = useQuery({
    queryKey: ['inspect', serverId, containerId, reveal],
    queryFn: () => containersApi.inspect(serverId, containerId, reveal),
    // Los valores revelados no se guardan en caché más de lo necesario
    gcTime: reveal ? 0 : undefined,
    staleTime: 30_000,
  })
  const env: string[] = data?.Config?.Env ?? []

  return (
    <Card>
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <CardTitle className="text-sm flex items-center gap-1">
            <KeyRound className="h-3.5 w-3.5" /> Variables de entorno
          </CardTitle>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              if (reveal || confirm('Los valores pueden incluir contraseñas y tokens. La consulta queda registrada. ¿Mostrarlos?')) {
                setReveal(r => !r)
              }
            }}
          >
            {reveal ? <EyeOff className="h-3.5 w-3.5 mr-1" /> : <Eye className="h-3.5 w-3.5 mr-1" />}
            {reveal ? 'Ocultar valores' : 'Mostrar valores'}
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="text-xs text-muted-foreground">Cargando…</div>
        ) : error ? (
          <div className="text-xs text-destructive">{apiErrorMessage(error, 'No se pudo consultar el contenedor')}</div>
        ) : env.length === 0 ? (
          <div className="text-xs text-muted-foreground">Sin variables de entorno.</div>
        ) : (
          <div className="font-mono text-xs space-y-0.5 max-h-64 overflow-auto">
            {env.map(e => {
              const i = e.indexOf('=')
              const [name, value] = i < 0 ? [e, ''] : [e.slice(0, i), e.slice(i + 1)]
              return (
                <div key={e} className="break-all">
                  <span className="text-primary">{name}</span>=<span className="text-slate-300">{value}</span>
                </div>
              )
            })}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
