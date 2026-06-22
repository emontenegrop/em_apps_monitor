import { useNavigate } from 'react-router-dom'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Server, RefreshCw, Trash2, Cpu, MemoryStick, Box } from 'lucide-react'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { serversApi } from '@/lib/api'
import { formatPercent, formatBytes, serverStatusColor } from '@/lib/utils'
import { useRealtimeHostMetrics, useRealtimeDockerInfo } from '@/hooks/useWebSocket'
import type { Server as ServerType } from '@/types'

interface ServerCardProps {
  server: ServerType
}

export function ServerCard({ server }: ServerCardProps) {
  const navigate = useNavigate()
  const qc = useQueryClient()

  const liveMetrics = useRealtimeHostMetrics(server.id)
  const liveDocker = useRealtimeDockerInfo(server.id)

  const pingMutation = useMutation({
    mutationFn: () => serversApi.ping(server.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['servers'] }),
  })

  const deleteMutation = useMutation({
    mutationFn: () => serversApi.delete(server.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['servers'] }),
  })

  const statusDot = serverStatusColor(server.status)

  return (
    <Card
      className="cursor-pointer hover:border-primary/50 transition-colors"
      onClick={() => navigate(`/servers/${server.id}`)}
    >
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className={`h-2 w-2 rounded-full ${statusDot}`} />
            <CardTitle className="text-base">{server.name}</CardTitle>
          </div>
          <div className="flex gap-1" onClick={e => e.stopPropagation()}>
            <Button
              size="icon"
              variant="ghost"
              className="h-7 w-7"
              disabled={pingMutation.isPending}
              onClick={() => pingMutation.mutate()}
              title="Verificar conexión"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${pingMutation.isPending ? 'animate-spin' : ''}`} />
            </Button>
            <Button
              size="icon"
              variant="ghost"
              className="h-7 w-7 hover:text-destructive-foreground"
              onClick={() => {
                if (confirm(`¿Eliminar servidor "${server.name}"?`)) deleteMutation.mutate()
              }}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
        <div className="flex gap-2 mt-1">
          <Badge variant="outline" className="text-xs">
            <Server className="h-3 w-3 mr-1" />
            {server.connection_type === 'local' ? 'Local' : server.host}
          </Badge>
          <Badge variant={server.status === 'online' ? 'success' : server.status === 'offline' ? 'destructive' : 'warning'}>
            {server.status}
          </Badge>
        </div>
      </CardHeader>

      <CardContent className="grid grid-cols-3 gap-3 text-center">
        {/* CPU */}
        <div className="bg-muted/40 rounded-md p-2">
          <Cpu className="h-4 w-4 mx-auto text-blue-400 mb-1" />
          <div className="text-lg font-bold text-blue-400">
            {formatPercent(liveMetrics?.cpu_usage_percent, 0)}
          </div>
          <div className="text-xs text-muted-foreground">CPU</div>
        </div>

        {/* RAM */}
        <div className="bg-muted/40 rounded-md p-2">
          <MemoryStick className="h-4 w-4 mx-auto text-purple-400 mb-1" />
          <div className="text-lg font-bold text-purple-400">
            {formatPercent(liveMetrics?.memory_usage_percent, 0)}
          </div>
          <div className="text-xs text-muted-foreground">
            {formatBytes(liveMetrics?.memory_used_bytes)}
          </div>
        </div>

        {/* Docker containers */}
        <div className="bg-muted/40 rounded-md p-2">
          <Box className="h-4 w-4 mx-auto text-cyan-400 mb-1" />
          <div className="text-lg font-bold text-cyan-400">
            {liveDocker?.containers_running ?? '—'}
          </div>
          <div className="text-xs text-muted-foreground">
            / {liveDocker?.containers_total ?? '—'} ctr
          </div>
        </div>
      </CardContent>
    </Card>
  )
}
