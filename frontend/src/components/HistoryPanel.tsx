import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Box, History as HistoryIcon, Loader2 } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { HistoryChart, toHistorySeries, type HistorySeries } from '@/components/HistoryChart'
import { metricsApi } from '@/lib/api'
import { formatBytes, formatRate } from '@/lib/utils'
import type { ContainerHistoryPoint, HistoryRange, HostHistoryPoint, Server } from '@/types'

type Metric = 'memory' | 'disk' | 'cpu'

const METRICS: { id: Metric; label: string }[] = [
  { id: 'memory', label: 'Memoria' },
  { id: 'disk', label: 'Disco' },
  { id: 'cpu', label: 'CPU' },
]

const RANGES: { id: HistoryRange; label: string }[] = [
  { id: '24h', label: '24 h' },
  { id: '3d', label: '3 días' },
  { id: '5d', label: '5 días' },
  { id: '7d', label: '7 días' },
]

const PALETTE = [
  '#60a5fa', '#a78bfa', '#34d399', '#f97316', '#f472b6',
  '#facc15', '#22d3ee', '#f87171', '#4ade80', '#c084fc',
]

const pct = (used: number | null, total: number | null) =>
  used != null && total ? (used / total) * 100 : null

const fmtPct = (v: number) => `${v.toFixed(0)}%`
const fmtBytes = (v: number) => formatBytes(v, 1)
const fmtRate = (v: number) => formatRate(v, 1)

function groupBy<T>(items: T[], key: (t: T) => string): Map<string, T[]> {
  const map = new Map<string, T[]>()
  for (const it of items) {
    const k = key(it)
    const list = map.get(k)
    if (list) list.push(it)
    else map.set(k, [it])
  }
  return map
}

function ChartBlock({ title, loading, empty, children }: {
  title: string
  loading: boolean
  empty: boolean
  children: React.ReactNode
}) {
  return (
    <div>
      <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1">{title}</div>
      {loading ? (
        <div className="h-[260px] flex items-center justify-center text-muted-foreground">
          <Loader2 className="h-5 w-5 animate-spin" />
        </div>
      ) : empty ? (
        <div className="h-[120px] flex items-center justify-center text-sm text-muted-foreground">
          Sin datos en el período.
        </div>
      ) : (
        children
      )}
    </div>
  )
}

/** Series por servidor: con todos, el % de cada uno; con uno, usado vs total */
function hostCharts(
  metric: Metric,
  points: HostHistoryPoint[],
  bucket: number,
  servers: Server[],
  single: boolean,
): { series: HistorySeries[]; yFormatter: (v: number) => string; yMax?: number } {
  const used = (p: HostHistoryPoint) => (metric === 'memory' ? p.memory_used_bytes : p.disk_used_bytes)
  const total = (p: HostHistoryPoint) => (metric === 'memory' ? p.memory_total_bytes : p.disk_total_bytes)

  if (single) {
    if (metric === 'cpu') {
      return {
        series: [{ name: 'CPU', color: PALETTE[0], data: toHistorySeries(points, bucket, p => p.cpu_percent) }],
        yFormatter: fmtPct,
        yMax: 100,
      }
    }
    const rows = points.filter(p => used(p) != null)
    return {
      series: [
        {
          name: 'Usada',
          color: metric === 'memory' ? '#a78bfa' : '#34d399',
          data: toHistorySeries(rows, bucket, used, p => `${fmtPct(pct(used(p), total(p)) ?? 0)}`),
        },
        { name: 'Total del servidor', color: '#94a3b8', dashed: true, data: toHistorySeries(rows, bucket, total) },
      ],
      yFormatter: fmtBytes,
    }
  }

  const byServer = groupBy(points, p => p.server_id)
  const series = servers
    .filter(s => byServer.has(s.id))
    .map((s, i) => {
      const rows = byServer.get(s.id)!
      return {
        name: s.name,
        color: PALETTE[i % PALETTE.length],
        data:
          metric === 'cpu'
            ? toHistorySeries(rows, bucket, p => p.cpu_percent)
            : toHistorySeries(
                rows.filter(p => used(p) != null),
                bucket,
                p => pct(used(p), total(p)),
                p => `${fmtBytes(used(p)!)} / ${fmtBytes(total(p) ?? 0)}`,
              ),
      }
    })
  return { series, yFormatter: fmtPct, yMax: 100 }
}

function containerSeries(metric: Metric, points: ContainerHistoryPoint[], bucket: number): HistorySeries[] {
  const value = (p: ContainerHistoryPoint) =>
    metric === 'cpu' ? p.cpu_percent : metric === 'memory' ? p.memory_used_bytes : p.disk_io_bytes
  return [...groupBy(points, p => p.container_id).values()]
    .map(rows => ({ name: rows[0].name, rows }))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map(({ name, rows }, i) => ({
      name,
      color: PALETTE[i % PALETTE.length],
      data: toHistorySeries(rows, bucket, value),
    }))
}

const CONTAINER_TITLES: Record<Metric, string> = {
  memory: 'Memoria usada por contenedor',
  disk: 'I/O de disco por contenedor (lectura + escritura)',
  cpu: 'CPU por contenedor',
}

const CONTAINER_FORMAT: Record<Metric, (v: number) => string> = {
  memory: fmtBytes,
  disk: fmtRate,
  cpu: fmtPct,
}

/** Gráficas históricas del Dashboard: servidores y, al elegir uno, sus contenedores */
export function HistoryPanel({ servers }: { servers: Server[] }) {
  const [metrics, setMetrics] = useState<Metric[]>(['memory'])
  const [range, setRange] = useState<HistoryRange>('24h')
  const [serverId, setServerId] = useState<string>('all')

  const single = serverId !== 'all'
  const selected = servers.find(s => s.id === serverId)

  const hosts = useQuery({
    queryKey: ['history', range, serverId],
    queryFn: () => metricsApi.hostHistory(range, single ? serverId : undefined),
    refetchInterval: 60_000,
  })
  const containers = useQuery({
    queryKey: ['containers-history', serverId, range],
    queryFn: () => metricsApi.containersHistory(serverId, range),
    enabled: single,
    refetchInterval: 60_000,
  })

  function toggle(m: Metric) {
    setMetrics(cur => {
      if (cur.includes(m)) return cur.length > 1 ? cur.filter(x => x !== m) : cur
      // Se conserva el orden de METRICS
      return METRICS.map(x => x.id).filter(x => x === m || cur.includes(x))
    })
  }

  const hostPoints = hosts.data?.points ?? []
  const hostBucket = hosts.data?.bucket_secs ?? 300
  const ctrPoints = containers.data?.points ?? []
  const ctrBucket = containers.data?.bucket_secs ?? 300
  const rangeLabel = RANGES.find(r => r.id === range)!.label

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm flex items-center gap-2">
            <HistoryIcon className="h-4 w-4 text-primary" />
            Historial — {selected ? selected.name : 'todos los servidores'} ({rangeLabel})
          </CardTitle>
          <div className="flex flex-wrap items-center gap-x-6 gap-y-3 pt-2">
            <div className="flex flex-wrap items-center gap-3">
              {METRICS.map(m => (
                <label key={m.id} className="flex items-center gap-1.5 text-sm cursor-pointer select-none">
                  <input
                    type="checkbox"
                    className="h-4 w-4 accent-primary"
                    checked={metrics.includes(m.id)}
                    onChange={() => toggle(m.id)}
                  />
                  {m.label}
                </label>
              ))}
            </div>

            <div className="flex flex-wrap gap-1">
              {RANGES.map(r => (
                <Button
                  key={r.id}
                  size="sm"
                  variant={range === r.id ? 'default' : 'outline'}
                  onClick={() => setRange(r.id)}
                >
                  {r.label}
                </Button>
              ))}
            </div>

            <select
              value={serverId}
              onChange={e => setServerId(e.target.value)}
              className="h-9 w-full sm:w-auto sm:min-w-[14rem] rounded-md border border-input bg-background px-3 text-sm focus:outline-none focus:ring-2 focus:ring-primary"
              aria-label="Servidor"
            >
              <option value="all">Todos los servidores</option>
              {servers.map(s => (
                <option key={s.id} value={s.id}>{s.name}</option>
              ))}
            </select>
          </div>
        </CardHeader>
        <CardContent className="space-y-6">
          {hosts.isError && <p className="text-sm text-destructive">No se pudo cargar el historial.</p>}
          {metrics.map(m => {
            const { series, yFormatter, yMax } = hostCharts(m, hostPoints, hostBucket, servers, single)
            const title =
              m === 'cpu'
                ? 'CPU'
                : single
                  ? `${m === 'memory' ? 'Memoria' : 'Disco'} usada vs total del servidor`
                  : `${m === 'memory' ? 'Memoria' : 'Disco'} usada (% del total de cada servidor)`
            return (
              <ChartBlock
                key={m}
                title={title}
                loading={hosts.isLoading}
                empty={series.every(s => !s.data.some(p => p.value[1] != null))}
              >
                <HistoryChart series={series} yFormatter={yFormatter} yMax={yMax} />
              </ChartBlock>
            )
          })}
        </CardContent>
      </Card>

      {single && selected && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm flex items-center gap-2">
              <Box className="h-4 w-4 text-cyan-400" />
              Contenedores de {selected.name} ({rangeLabel})
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-6">
            {containers.isError && <p className="text-sm text-destructive">No se pudo cargar el historial.</p>}
            {metrics.map(m => {
              const series = containerSeries(m, ctrPoints, ctrBucket)
              return (
                <ChartBlock
                  key={m}
                  title={CONTAINER_TITLES[m]}
                  loading={containers.isLoading}
                  empty={series.length === 0}
                >
                  <HistoryChart series={series} yFormatter={CONTAINER_FORMAT[m]} height={300} />
                </ChartBlock>
              )
            })}
          </CardContent>
        </Card>
      )}
    </div>
  )
}
