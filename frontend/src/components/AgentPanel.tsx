import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, Bot, KeyRound, ShieldOff } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { InstallCommand } from '@/components/InstallCommand'
import { serversApi, type EnrollmentToken } from '@/lib/api'
import { apiErrorMessage } from '@/lib/auth'
import { agentState, agentStateBadge, agentStateLabel } from '@/lib/agent'
import { runtimeLabel, timeAgo } from '@/lib/utils'
import type { Server } from '@/types'

/** Estado del agente del servidor y acciones de inscripción / revocación */
export function AgentPanel({ server }: { server: Server }) {
  const qc = useQueryClient()
  const [enrollment, setEnrollment] = useState<EnrollmentToken | null>(null)
  const state = agentState(server)
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ['server', server.id] })
    qc.invalidateQueries({ queryKey: ['servers'] })
  }

  const token = useMutation({
    mutationFn: () => serversApi.enrollmentToken(server.id),
    onSuccess: t => {
      setEnrollment(t)
      refresh()
    },
  })
  const revoke = useMutation({
    mutationFn: () => serversApi.revokeAgent(server.id),
    onSuccess: refresh,
  })
  const error = token.error ?? revoke.error

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <CardTitle className="text-sm flex items-center gap-2">
            <Bot className="h-4 w-4 text-primary" /> Agente
            <Badge variant={agentStateBadge[state]}>{agentStateLabel[state]}</Badge>
          </CardTitle>
          <div className="flex gap-2 flex-wrap">
            <Button size="sm" variant="outline" disabled={token.isPending} onClick={() => token.mutate()}>
              <KeyRound className="h-3.5 w-3.5 mr-1" />
              {state === 'pending' || state === 'expired' || state === 'revoked'
                ? 'Generar ficha de instalación'
                : 'Reinstalar (nueva ficha)'}
            </Button>
            {server.agent_enrolled && (
              <Button
                size="sm"
                variant="outline"
                className="hover:text-destructive-foreground"
                disabled={revoke.isPending}
                onClick={() => {
                  if (confirm(`¿Revocar la credencial del agente de "${server.name}"? Se desconectará y habrá que inscribirlo de nuevo.`)) {
                    revoke.mutate()
                  }
                }}
              >
                <ShieldOff className="h-3.5 w-3.5 mr-1" />
                Revocar
              </Button>
            )}
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {error && (
          <p className="text-xs text-destructive-foreground bg-destructive/20 p-2 rounded">
            {apiErrorMessage(error, 'La operación falló')}
          </p>
        )}

        {state === 'pending' && (
          <p className="text-sm text-muted-foreground">
            Hay una ficha de instalación pendiente que vence {new Date(server.enrollment_expires_at!).toLocaleString()}.
          </p>
        )}
        {state === 'revoked' && (
          <p className="text-sm text-muted-foreground">
            La credencial del agente fue revocada. Para volver a monitorear este host, genera una ficha e inscríbelo de nuevo.
          </p>
        )}
        {state === 'expired' && (
          <p className="text-sm text-muted-foreground">
            El agente todavía no se inscribió. Genera una ficha e instálalo en el host.
          </p>
        )}

        {server.last_connected_at && (
          <dl className="grid grid-cols-2 sm:grid-cols-4 gap-x-6 gap-y-2 text-sm">
            {[
              ['Host', server.hostname],
              ['Sistema', server.os],
              ['Arquitectura', server.arch],
              ['Versión del agente', server.agent_version],
              ['Última conexión', timeAgo(server.last_connected_at)],
              ['Último dato', timeAgo(server.last_seen_at)],
            ].map(([k, v]) => (
              <div key={k} className="min-w-0">
                <dt className="text-muted-foreground text-xs">{k}</dt>
                <dd className="font-medium truncate" title={v ?? undefined}>{v ?? '—'}</dd>
              </div>
            ))}
          </dl>
        )}

        {server.runtimes?.length > 0 && (
          <div className="space-y-2">
            {server.runtimes.map(r => (
              <div key={`${r.kind}-${r.socket}`} className="flex items-start gap-3 text-sm rounded-md bg-muted/40 p-2">
                <Badge variant={r.error ? 'warning' : 'outline'}>{runtimeLabel(r.kind)}</Badge>
                <div className="min-w-0">
                  <div className="break-all">
                    {r.version ?? '—'}
                    {r.rootless && <span className="text-muted-foreground"> · rootless</span>}
                    <span className="text-muted-foreground font-mono text-xs"> · {r.socket}</span>
                  </div>
                  {r.error && (
                    <div className="text-xs text-yellow-400 flex items-start gap-1 mt-0.5">
                      <AlertTriangle className="h-3 w-3 mt-0.5 shrink-0" />
                      <span className="break-all">{r.error}</span>
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>

      <Dialog open={!!enrollment} onOpenChange={o => !o && setEnrollment(null)}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Instalar el agente en "{server.name}"</DialogTitle>
          </DialogHeader>
          {server.agent_enrolled && (
            <p className="text-xs text-muted-foreground">
              El agente actual sigue funcionando hasta que se use esta ficha; al usarla, su credencial anterior deja de valer.
            </p>
          )}
          {enrollment && <InstallCommand enrollment={enrollment} />}
        </DialogContent>
      </Dialog>
    </Card>
  )
}
