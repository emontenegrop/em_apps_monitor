import { useState } from 'react'
import { useParams, Link } from 'react-router-dom'
import { useMutation, useQuery } from '@tanstack/react-query'
import { subHours } from 'date-fns'
import {
  ArrowLeft, Box, HardDrive, Cpu, MemoryStick,
  Activity, RefreshCw,
} from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { AgentPanel } from '@/components/AgentPanel'
import { ContainerCard } from '@/components/ContainerCard'
import { MetricsChart } from '@/components/MetricsChart'
import { RuntimeBadges } from '@/components/RuntimeBadges'
import { serversApi, containersApi, metricsApi } from '@/lib/api'
import { agentState, agentStateBadge, agentStateLabel } from '@/lib/agent'
import {
  formatBytes, formatPercent, formatRate, formatUptime, runtimeLabel, serverStatusColor,
} from '@/lib/utils'
import { useRealtimeHostMetrics, useRealtimeRuntimeInfo, sumContainers } from '@/hooks/useWebSocket'
import type { DockerInfo, RuntimeKind } from '@/types'

export function ServerDetail() {
  const { id } = useParams<{ id: string }>()

  const [runtimeFilter, setRuntimeFilter] = useState<RuntimeKind | 'all'>('all')

  const { data: server, refetch: refetchServer } = useQuery({
    queryKey: ['server', id],
    queryFn: () => serversApi.get(id!),
    enabled: !!id,
    refetchInterval: 15_000,
  })

  const { data: runtimeRows = [] } = useQuery({
    queryKey: ['runtimes', id],
    queryFn: () => metricsApi.runtimes(id!),
    enabled: !!id,
    refetchInterval: 60_000,
  })

  const ping = useMutation({
    mutationFn: () => serversApi.ping(id!),
    onSuccess: () => refetchServer(),
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
  const liveRuntimes = useRealtimeRuntimeInfo(id)

  if (!server) return <div className="p-6 text-muted-foreground">Cargando…</div>

  // Info por runtime: la del WebSocket si ya llegó, si no la última guardada
  const runtimeInfos: [RuntimeKind, DockerInfo][] = [
    ...runtimeRows.map(r => [r.runtime_kind, liveRuntimes[r.runtime_kind] ?? r] as [RuntimeKind, DockerInfo]),
    ...(Object.entries(liveRuntimes) as [RuntimeKind, DockerInfo][])
      .filter(([k]) => !runtimeRows.some(r => r.runtime_kind === k)),
  ]
  const counts = sumContainers(Object.fromEntries(runtimeInfos))
  const kinds = [...new Set(containers.map(c => c.runtime_kind))]
  const multiRuntime = kinds.length > 1
  const visible = containers.filter(c => runtimeFilter === 'all' || c.runtime_kind === runtimeFilter)
  const running = visible.filter(c => c.state === 'running')
  const stopped = visible.filter(c => c.state !== 'running')
  const state = agentState(server)

  const timestamps = hostHistory.map(m => m.time)
  const cpuData = hostHistory.map(m => m.cpu_usage_percent ?? 0)
  const memData = hostHistory.map(m => m.memory_usage_percent ?? 0)
  // La red llega como tasa (bytes/s)
  const netRx = hostHistory.map(m => m.net_rx_bytes ?? 0)
  const netTx = hostHistory.map(m => m.net_tx_bytes ?? 0)

  return (
    <div className="p-4 md:p-6 space-y-4 md:space-y-6">
      {/* Header */}
      <div className="flex items-start sm:items-center gap-3 flex-wrap">
        <Link to="/">
          <Button variant="ghost" size="icon" className="h-8 w-8">
            <ArrowLeft className="h-4 w-4" />
          </Button>
        </Link>
        <div className="flex items-center gap-2 flex-wrap min-w-0 flex-1">
          <span className={`h-2.5 w-2.5 shrink-0 rounded-full ${serverStatusColor(server.status)}`} />
          <h1 className="text-lg md:text-xl font-bold break-all">{server.name}</h1>
          <Badge variant="outline" className="max-w-full truncate">{server.hostname ?? 'sin inscribir'}</Badge>
          <Badge variant={agentStateBadge[state]}>{agentStateLabel[state]}</Badge>
          <RuntimeBadges runtimes={server.runtimes} showVersion />
        </div>
        <div className="ml-auto flex items-center gap-2">
          {ping.data && (
            <span className="text-xs text-muted-foreground">
              {ping.data.status === 'online'
                ? ping.data.latency_ms != null ? `${ping.data.latency_ms} ms` : 'en línea'
                : ping.data.error ?? 'fuera de línea'}
            </span>
          )}
          <Button variant="outline" size="sm" disabled={ping.isPending} onClick={() => ping.mutate()}>
            <RefreshCw className={`h-3.5 w-3.5 mr-1 ${ping.isPending ? 'animate-spin' : ''}`} />
            Ping
          </Button>
        </div>
      </div>

      <AgentPanel server={server} />

      {/* Live stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3 md:gap-4">
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
              {counts?.running ?? containers.filter(c => c.state === 'running').length}
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">
              en ejecución / {counts?.total ?? containers.length} total
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
            <div className="text-xs text-muted-foreground mt-0.5 truncate">
              {runtimeInfos.length
                ? runtimeInfos.map(([k, i]) => `${runtimeLabel(k)} ${i.docker_version ?? ''}`).join(' · ')
                : '—'}
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
              <CardTitle className="text-sm">
                Red (1h)
              </CardTitle>
            </CardHeader>
            <CardContent>
              <MetricsChart
                timestamps={timestamps}
                series={[
                  { name: 'Recibido', data: netRx, color: '#34d399' },
                  { name: 'Enviado', data: netTx, color: '#f97316' },
                ]}
                yFormatter={v => formatRate(v)}
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

      {/* Info de cada runtime */}
      {runtimeInfos.map(([kind, info]) => (
        <div key={kind}>
          <h2 className="text-sm font-semibold text-muted-foreground mb-3 uppercase tracking-wider flex items-center gap-1">
            <Box className="h-3.5 w-3.5" /> {runtimeLabel(kind)}
          </h2>
          <Card>
            <CardContent className="p-4">
              <dl className="grid grid-cols-2 sm:grid-cols-4 gap-x-6 gap-y-2 text-sm">
                {[
                  ['Versión', info.docker_version],
                  ['API', info.api_version],
                  ['OS', info.os_type],
                  ['Arch', info.arch],
                  ['Kernel', info.kernel_version],
                  ['Storage', info.storage_driver],
                  ['Imágenes', info.images_total],
                  ...(kind === 'docker' ? [['Swarm', info.swarm_active ? 'activo' : 'inactivo']] : []),
                ].map(([k, v]) => (
                  <div key={String(k)}>
                    <dt className="text-muted-foreground text-xs">{k}</dt>
                    <dd className="font-medium truncate">{v ?? '—'}</dd>
                  </div>
                ))}
              </dl>
            </CardContent>
          </Card>
        </div>
      ))}

      {/* Containers */}
      <div>
        <div className="flex items-center justify-between mb-3 gap-2 flex-wrap">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-1">
            <Box className="h-3.5 w-3.5" /> Contenedores ({visible.length})
          </h2>
          {multiRuntime && (
            <div className="flex gap-1 flex-wrap">
              {(['all', ...kinds] as const).map(k => (
                <Button
                  key={k}
                  size="sm"
                  variant={runtimeFilter === k ? 'default' : 'outline'}
                  onClick={() => setRuntimeFilter(k)}
                >
                  {k === 'all' ? 'Todos' : runtimeLabel(k)}
                </Button>
              ))}
            </div>
          )}
        </div>
        {running.length > 0 && (
          <>
            <div className="text-xs text-green-400 mb-2">En ejecución ({running.length})</div>
            <div className="grid sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3 mb-4">
              {running.map(c => (
                <ContainerCard key={`${c.runtime_kind}-${c.id}`} container={c} serverId={server.id} showRuntime={multiRuntime} />
              ))}
            </div>
          </>
        )}
        {stopped.length > 0 && (
          <>
            <div className="text-xs text-muted-foreground mb-2">Detenidos ({stopped.length})</div>
            <div className="grid sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3">
              {stopped.map(c => (
                <ContainerCard key={`${c.runtime_kind}-${c.id}`} container={c} serverId={server.id} showRuntime={multiRuntime} />
              ))}
            </div>
          </>
        )}
        {visible.length === 0 && (
          <div className="text-center py-8 text-muted-foreground">
            <Box className="h-8 w-8 mx-auto mb-2 opacity-30" />
            <p className="text-sm">No se encontraron contenedores.</p>
          </div>
        )}
      </div>
    </div>
  )
}
