import { useNavigate } from 'react-router-dom'
import { Box, Cpu, MemoryStick, Network } from 'lucide-react'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { formatPercent, formatBytes, formatRate, containerStateColor, runtimeLabel } from '@/lib/utils'
import { useRealtimeContainerMetrics } from '@/hooks/useWebSocket'
import type { Container } from '@/types'

interface ContainerCardProps {
  container: Container
  serverId: string
  /** Mostrar el runtime (cuando el host tiene Docker y Podman) */
  showRuntime?: boolean
  /** La red llega como tasa en bytes/s (servidores con agente) */
  ratesPerSecond?: boolean
}

export function ContainerCard({ container, serverId, showRuntime = false, ratesPerSecond = false }: ContainerCardProps) {
  const navigate = useNavigate()
  const metrics = useRealtimeContainerMetrics(serverId, container.id)

  const stateClass = containerStateColor(container.state)

  return (
    <Card
      className="cursor-pointer hover:border-primary/40 transition-colors"
      onClick={() => navigate(`/servers/${serverId}/containers/${container.id}`)}
    >
      <CardContent className="p-4">
        <div className="flex items-start justify-between gap-2 mb-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <Box className={`h-4 w-4 shrink-0 ${stateClass}`} />
              <span className="font-semibold text-sm truncate">{container.name ?? container.short_id}</span>
            </div>
            <div className="text-xs text-muted-foreground mt-0.5 truncate">
              {showRuntime && <span className="text-primary">{runtimeLabel(container.runtime_kind)} · </span>}
              {container.image}
            </div>
          </div>
          <Badge
            variant={
              container.state === 'running'
                ? 'success'
                : container.state === 'exited'
                ? 'destructive'
                : 'warning'
            }
            className="shrink-0"
          >
            {container.state}
          </Badge>
        </div>

        <div className="grid grid-cols-3 gap-2 text-xs">
          <div className="flex items-center gap-1 text-muted-foreground">
            <Cpu className="h-3 w-3 text-blue-400" />
            <span>{formatPercent(metrics?.cpu_usage_percent, 1)}</span>
          </div>
          <div className="flex items-center gap-1 text-muted-foreground">
            <MemoryStick className="h-3 w-3 text-purple-400" />
            <span>{formatBytes(metrics?.memory_used_bytes)}</span>
          </div>
          <div className="flex items-center gap-1 text-muted-foreground">
            <Network className="h-3 w-3 text-cyan-400" />
            <span>{ratesPerSecond ? formatRate(metrics?.net_rx_bytes) : formatBytes(metrics?.net_rx_bytes)}↓</span>
          </div>
        </div>

        {container.networks?.length > 0 && (
          <div className="flex flex-wrap gap-1 mt-2">
            {container.networks.map(n => (
              <Badge key={n} variant="outline" className="text-xs py-0">
                {n}
              </Badge>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
