import { useParams, Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { subHours } from 'date-fns'
import {
  ArrowLeft, Server, Box, HardDrive, Network, Cpu, MemoryStick,
  Activity, RefreshCw,
} from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ContainerCard } from '@/components/ContainerCard'
import { MetricsChart } from '@/components/MetricsChart'
import { serversApi, containersApi, metricsApi } from '@/lib/api'
import {
  formatBytes, formatPercent, formatUptime, serverStatusColor,
} from '@/lib/utils'
import { useRealtimeHostMetrics, useRealtimeDockerInfo } from '@/hooks/useWebSocket'

export function ServerDetail() {
  const { id } = useParams<{ id: string }>()

  const { data: server, refetch: refetchServer } = useQuery({
    queryKey: ['server', id],
    queryFn: () => serversApi.get(id!),
    enabled: !!id,
  })

  const { data: containers = [] } = useQuery({
    queryKey: ['containers', id],
    queryFn: () => containersApi.list(id!),
    enabled: !!id,
    refetchInterval: 15_000,
  })

  const { data: disks = [] } = useQuery({
    queryKey: ['disks', id],
    queryFn: () => metricsApi.disks(id!),
    enabled: !!id,
    refetchInterval: 60_000,
  })

  const from = subHours(new Date(), 1).toISOString()
  const { data: hostHistory = [] } = useQuery({
    queryKey: ['host-metrics', id, from],
    queryFn: () => metricsApi.host(id!, from),
    enabled: !!id,
    refetchInterval: 15_000,
  })

  const liveHost = useRealtimeHostMetrics(id)
  const liveDocker = useRealtimeDockerInfo(id)

  if (!server) return <div className="p-6 text-muted-foreground">Cargando…</div>

  const running = containers.filter(c => c.state === 'running')
  const stopped = containers.filter(c => c.state !== 'running')

  const timestamps = hostHistory.map(m => m.time)
  const cpuData = hostHistory.map(m => m.cpu_usage_percent ?? 0)
  const memData = hostHistory.map(m => m.memory_usage_percent ?? 0)
  const netRx = hostHistory.map(m => (m.net_rx_bytes ?? 0) / 1024 / 1024)
  const netTx = hostHistory.map(m => (m.net_tx_bytes ?? 0) / 1024 / 1024)

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <Link to="/">
          <Button variant="ghost" size="icon" className="h-8 w-8">
            <ArrowLeft className="h-4 w-4" />
          </Button>
        </Link>
        <div className="flex items-center gap-2">
          <span className={`h-2.5 w-2.5 rounded-full ${serverStatusColor(server.status)}`} />
          <h1 className="text-xl font-bold">{server.name}</h1>
          <Badge variant="outline">{server.host}</Badge>
          <Badge variant={server.status === 'online' ? 'success' : 'destructive'}>
            {server.status}
          </Badge>
        </div>
        <Button
          variant="outline"
          size="sm"
          className="ml-auto"
          onClick={() => serversApi.ping(server.id).then(() => refetchServer())}
        >
          <RefreshCw className="h-3.5 w-3.5 mr-1" />
          Ping
        </Button>
      </div>

      {/* Docker info + live stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="p-4">
            <div className="text-xs text-muted-foreground mb-1 flex items-center gap-1">
              <Cpu className="h-3 w-3 text-blue-400" /> CPU
            </div>
            <div className="text-2xl font-bold text-blue-400">
              {formatPercent(liveHost?.cpu_usage_percent, 0)}
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">
              Load: {liveHost?.load_avg_1?.toFixed(2) ?? '—'}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4">
            <div className="text-xs text-muted-foreground mb-1 flex items-center gap-1">
              <MemoryStick className="h-3 w-3 text-purple-400" /> Memoria
            </div>
            <div className="text-2xl font-bold text-purple-400">
              {formatPercent(liveHost?.memory_usage_percent, 0)}
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">
              {formatBytes(liveHost?.memory_used_bytes)} / {formatBytes(liveHost?.memory_total_bytes)}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4">
            <div className="text-xs text-muted-foreground mb-1 flex items-center gap-1">
              <Box className="h-3 w-3 text-cyan-400" /> Contenedores
            </div>
            <div className="text-2xl font-bold text-cyan-400">
              {liveDocker?.containers_running ?? running.length}
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">
              running / {liveDocker?.containers_total ?? containers.length} total
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4">
            <div className="text-xs text-muted-foreground mb-1 flex items-center gap-1">
              <Activity className="h-3 w-3 text-green-400" /> Uptime
            </div>
            <div className="text-xl font-bold text-green-400">
              {formatUptime(liveHost?.uptime_seconds)}
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">
              {liveDocker?.docker_version ? `Docker ${liveDocker.docker_version}` : '—'}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Charts */}
      {hostHistory.length > 0 && (
        <div className="grid md:grid-cols-2 gap-4">
          <Card>
            <CardHeader className="pb-0">
              <CardTitle className="text-sm">CPU & Memoria (1h)</CardTitle>
            </CardHeader>
            <CardContent>
              <MetricsChart
                timestamps={timestamps}
                series={[
                  { name: 'CPU %', data: cpuData, color: '#60a5fa' },
                  { name: 'RAM %', data: memData, color: '#a78bfa' },
                ]}
                yMax={100}
                yFormatter={v => `${v.toFixed(0)}%`}
              />
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-0">
              <CardTitle className="text-sm">Red (MB acumulado)</CardTitle>
            </CardHeader>
            <CardContent>
              <MetricsChart
                timestamps={timestamps}
                series={[
                  { name: 'RX MB', data: netRx, color: '#34d399' },
                  { name: 'TX MB', data: netTx, color: '#f97316' },
                ]}
                yFormatter={v => `${v.toFixed(1)} MB`}
              />
            </CardContent>
          </Card>
        </div>
      )}

      {/* Disks */}
      {disks.length > 0 && (
        <div>
          <h2 className="text-sm font-semibold text-muted-foreground mb-3 uppercase tracking-wider flex items-center gap-1">
            <HardDrive className="h-3.5 w-3.5" /> Discos
          </h2>
          <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {disks.map(d => (
              <Card key={d.mount_point}>
                <CardContent className="p-4">
                  <div className="flex justify-between items-start mb-2">
                    <div>
                      <div className="font-medium text-sm">{d.mount_point}</div>
                      <div className="text-xs text-muted-foreground">{d.name} · {d.fs_type}</div>
                    </div>
                    <span className={`text-sm font-bold ${d.usage_percent > 85 ? 'text-red-400' : d.usage_percent > 70 ? 'text-yellow-400' : 'text-green-400'}`}>
                      {d.usage_percent.toFixed(0)}%
                    </span>
                  </div>
                  <div className="h-1.5 bg-muted rounded-full overflow-hidden">
                    <div
                      className={`h-full rounded-full ${d.usage_percent > 85 ? 'bg-red-500' : d.usage_percent > 70 ? 'bg-yellow-500' : 'bg-green-500'}`}
                      style={{ width: `${d.usage_percent}%` }}
                    />
                  </div>
                  <div className="text-xs text-muted-foreground mt-1">
                    {formatBytes(d.used_bytes)} / {formatBytes(d.total_bytes)}
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      )}

      {/* Docker info */}
      {liveDocker && (
        <div>
          <h2 className="text-sm font-semibold text-muted-foreground mb-3 uppercase tracking-wider flex items-center gap-1">
            <Box className="h-3.5 w-3.5" /> Docker
          </h2>
          <Card>
            <CardContent className="p-4">
              <dl className="grid grid-cols-2 sm:grid-cols-4 gap-x-6 gap-y-2 text-sm">
                {[
                  ['Versión', liveDocker.docker_version],
                  ['API', liveDocker.api_version],
                  ['OS', liveDocker.os_type],
                  ['Arch', liveDocker.arch],
                  ['Kernel', liveDocker.kernel_version],
                  ['Storage', liveDocker.storage_driver],
                  ['Imágenes', liveDocker.images_total],
                  ['Swarm', liveDocker.swarm_active ? 'activo' : 'inactivo'],
                ].map(([k, v]) => (
                  <div key={k}>
                    <dt className="text-muted-foreground text-xs">{k}</dt>
                    <dd className="font-medium truncate">{v ?? '—'}</dd>
                  </div>
                ))}
              </dl>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Containers */}
      <div>
        <h2 className="text-sm font-semibold text-muted-foreground mb-3 uppercase tracking-wider flex items-center gap-1">
          <Box className="h-3.5 w-3.5" /> Contenedores ({containers.length})
        </h2>
        {running.length > 0 && (
          <>
            <div className="text-xs text-green-400 mb-2">En ejecución ({running.length})</div>
            <div className="grid sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3 mb-4">
              {running.map(c => (
                <ContainerCard key={c.id} container={c} serverId={server.id} />
              ))}
            </div>
          </>
        )}
        {stopped.length > 0 && (
          <>
            <div className="text-xs text-muted-foreground mb-2">Detenidos ({stopped.length})</div>
            <div className="grid sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3">
              {stopped.map(c => (
                <ContainerCard key={c.id} container={c} serverId={server.id} />
              ))}
            </div>
          </>
        )}
        {containers.length === 0 && (
          <div className="text-center py-8 text-muted-foreground">
            <Box className="h-8 w-8 mx-auto mb-2 opacity-30" />
            <p className="text-sm">No se encontraron contenedores.</p>
          </div>
        )}
      </div>
    </div>
  )
}
