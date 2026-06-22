import { useParams, Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { subHours } from 'date-fns'
import { ArrowLeft, Box, Cpu, MemoryStick, Network, HardDrive, Terminal } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { MetricsChart } from '@/components/MetricsChart'
import { containersApi, metricsApi } from '@/lib/api'
import { formatBytes, formatPercent, containerStateColor } from '@/lib/utils'
import { useRealtimeContainerMetrics } from '@/hooks/useWebSocket'

export function ContainerDetail() {
  const { id: serverId, cid } = useParams<{ id: string; cid: string }>()

  const { data: container } = useQuery({
    queryKey: ['container', serverId, cid],
    queryFn: () => containersApi.get(serverId!, cid!),
    enabled: !!serverId && !!cid,
  })

  const { data: logs = [] } = useQuery({
    queryKey: ['logs', serverId, cid],
    queryFn: () => containersApi.logs(serverId!, cid!),
    enabled: !!serverId && !!cid,
    refetchInterval: 10_000,
  })

  const from = subHours(new Date(), 1).toISOString()
  const { data: history = [] } = useQuery({
    queryKey: ['container-metrics', cid, from],
    queryFn: () => metricsApi.container(serverId!, cid!, from),
    enabled: !!serverId && !!cid,
    refetchInterval: 15_000,
  })

  const live = useRealtimeContainerMetrics(serverId, cid)

  if (!container) return <div className="p-6 text-muted-foreground">Cargando…</div>

  const timestamps = history.map(m => m.time)
  const cpuData = history.map(m => m.cpu_usage_percent ?? 0)
  const memData = history.map(m => m.memory_usage_percent ?? 0)
  const netRx = history.map(m => (m.net_rx_bytes ?? 0) / 1024 / 1024)
  const netTx = history.map(m => (m.net_tx_bytes ?? 0) / 1024 / 1024)
  const blkR = history.map(m => (m.block_read_bytes ?? 0) / 1024 / 1024)
  const blkW = history.map(m => (m.block_write_bytes ?? 0) / 1024 / 1024)

  const stateClass = containerStateColor(container.state)

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <Link to={`/servers/${serverId}`}>
          <Button variant="ghost" size="icon" className="h-8 w-8">
            <ArrowLeft className="h-4 w-4" />
          </Button>
        </Link>
        <Box className={`h-5 w-5 ${stateClass}`} />
        <div>
          <h1 className="text-xl font-bold">{container.name ?? container.short_id}</h1>
          <div className="text-xs text-muted-foreground">{container.image}</div>
        </div>
        <Badge
          variant={container.state === 'running' ? 'success' : 'destructive'}
          className="ml-2"
        >
          {container.state}
        </Badge>
      </div>

      {/* Live metrics */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="p-4">
            <div className="text-xs text-muted-foreground mb-1 flex items-center gap-1">
              <Cpu className="h-3 w-3 text-blue-400" /> CPU
            </div>
            <div className="text-2xl font-bold text-blue-400">
              {formatPercent(live?.cpu_usage_percent)}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4">
            <div className="text-xs text-muted-foreground mb-1 flex items-center gap-1">
              <MemoryStick className="h-3 w-3 text-purple-400" /> Memoria
            </div>
            <div className="text-2xl font-bold text-purple-400">
              {formatPercent(live?.memory_usage_percent)}
            </div>
            <div className="text-xs text-muted-foreground">
              {formatBytes(live?.memory_used_bytes)} / {formatBytes(live?.memory_limit_bytes)}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4">
            <div className="text-xs text-muted-foreground mb-1 flex items-center gap-1">
              <Network className="h-3 w-3 text-cyan-400" /> Red
            </div>
            <div className="text-sm font-bold text-cyan-400">
              ↓ {formatBytes(live?.net_rx_bytes)}
            </div>
            <div className="text-sm font-bold text-orange-400">
              ↑ {formatBytes(live?.net_tx_bytes)}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4">
            <div className="text-xs text-muted-foreground mb-1 flex items-center gap-1">
              <HardDrive className="h-3 w-3 text-green-400" /> I/O Disco
            </div>
            <div className="text-sm font-bold text-green-400">
              R: {formatBytes(live?.block_read_bytes)}
            </div>
            <div className="text-sm font-bold text-yellow-400">
              W: {formatBytes(live?.block_write_bytes)}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Charts */}
      {history.length > 0 && (
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
                yFormatter={v => `${v.toFixed(1)}%`}
              />
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-0">
              <CardTitle className="text-sm">Red MB (acumulado)</CardTitle>
            </CardHeader>
            <CardContent>
              <MetricsChart
                timestamps={timestamps}
                series={[
                  { name: 'RX', data: netRx, color: '#34d399' },
                  { name: 'TX', data: netTx, color: '#f97316' },
                ]}
                yFormatter={v => `${v.toFixed(1)} MB`}
              />
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-0">
              <CardTitle className="text-sm">I/O Disco MB</CardTitle>
            </CardHeader>
            <CardContent>
              <MetricsChart
                timestamps={timestamps}
                series={[
                  { name: 'Read', data: blkR, color: '#4ade80' },
                  { name: 'Write', data: blkW, color: '#facc15' },
                ]}
                yFormatter={v => `${v.toFixed(1)} MB`}
              />
            </CardContent>
          </Card>

          {/* Container info */}
          <Card>
            <CardHeader className="pb-0">
              <CardTitle className="text-sm">Detalles</CardTitle>
            </CardHeader>
            <CardContent>
              <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
                {[
                  ['ID corto', container.short_id],
                  ['Estado', container.state],
                  ['PIDs', live?.pids],
                  ['Redes', (container.networks as string[])?.join(', ')],
                  ['Puertos', JSON.stringify(container.ports)],
                  ['Comando', container.command],
                ].map(([k, v]) => (
                  <div key={String(k)} className="col-span-2 sm:col-span-1">
                    <dt className="text-xs text-muted-foreground">{k}</dt>
                    <dd className="font-medium text-xs truncate">{String(v ?? '—')}</dd>
                  </div>
                ))}
              </dl>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Logs */}
      <div>
        <h2 className="text-sm font-semibold text-muted-foreground mb-3 uppercase tracking-wider flex items-center gap-1">
          <Terminal className="h-3.5 w-3.5" /> Logs (últimas 200 líneas)
        </h2>
        <Card>
          <CardContent className="p-0">
            <div className="font-mono text-xs overflow-auto max-h-96 p-4 space-y-0.5 bg-[#0d1117] rounded-lg">
              {logs.length === 0 ? (
                <span className="text-muted-foreground">Sin logs disponibles</span>
              ) : (
                logs.map((line, i) => (
                  <div key={i} className="whitespace-pre-wrap break-all text-slate-300 hover:bg-white/5 px-1 rounded">
                    {line}
                  </div>
                ))
              )}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
