import { useEffect, useMemo, useRef } from 'react'
import { Link } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import ReactECharts from 'echarts-for-react'
import { AlertTriangle, CheckCircle2, Globe, HeartPulse, Settings2, XCircle } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { HistoryChart, toHistorySeries, type HistorySeries } from '@/components/HistoryChart'
import { VitalSigns } from '@/components/VitalSigns'
import { ws } from '@/lib/websocket'
import { statusBadge, statusLabel, targetStatus, uptimeApi, type Target } from '@/lib/uptime'
import { timeAgo } from '@/lib/utils'

const PALETTE = [
  '#60a5fa', '#a78bfa', '#34d399', '#f97316', '#f472b6',
  '#facc15', '#22d3ee', '#f87171', '#4ade80', '#c084fc',
]

const AXIS = { color: '#64748b', fontSize: 10 }
const TOOLTIP = {
  backgroundColor: 'rgba(15, 23, 42, 0.95)',
  borderColor: '#334155',
  textStyle: { color: '#e2e8f0', fontSize: 11 },
  confine: true,
}

function esc(s: string) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function Kpi({ label, value, hint, icon: Icon, color }: {
  label: string
  value: number
  hint: string
  icon: React.ElementType
  color: string
}) {
  return (
    <Card>
      <CardContent className="flex items-center gap-3 p-3 md:p-4">
        <Icon className={`h-8 w-8 shrink-0 ${color}`} />
        <div className="min-w-0">
          <div className="text-xs text-muted-foreground truncate">{label}</div>
          <div className={`text-2xl font-bold ${color}`}>{value}</div>
          <div className="text-xs text-muted-foreground truncate">{hint}</div>
        </div>
      </CardContent>
    </Card>
  )
}

function ChartCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Card>
      <CardHeader className="pb-0">
        <CardTitle className="text-sm">{title}</CardTitle>
      </CardHeader>
      <CardContent className="pt-3">{children}</CardContent>
    </Card>
  )
}

function Empty({ text }: { text: string }) {
  return <div className="h-[160px] flex items-center justify-center text-sm text-muted-foreground">{text}</div>
}

/** Últimos 7 días (hoy incluido) como YYYY-MM-DD en la zona del navegador */
function lastSevenDays(): string[] {
  const out: string[] = []
  const d = new Date()
  for (let i = 6; i >= 0; i--) {
    const x = new Date(d.getFullYear(), d.getMonth(), d.getDate() - i)
    out.push(`${x.getFullYear()}-${String(x.getMonth() + 1).padStart(2, '0')}-${String(x.getDate()).padStart(2, '0')}`)
  }
  return out
}

function dayLabel(iso: string) {
  const [y, m, d] = iso.split('-').map(Number)
  return new Date(y, m - 1, d).toLocaleDateString([], { weekday: 'short', day: '2-digit', month: '2-digit' })
}

export function Uptime() {
  const qc = useQueryClient()

  const { data: targets = [], isLoading } = useQuery({
    queryKey: ['uptime', 'targets'],
    queryFn: uptimeApi.targets,
    refetchInterval: 30_000,
  })
  const { data: availability = [] } = useQuery({
    queryKey: ['uptime', 'availability'],
    queryFn: uptimeApi.availability,
    refetchInterval: 60_000,
  })
  const { data: times } = useQuery({
    queryKey: ['uptime', 'response-times'],
    queryFn: uptimeApi.responseTimes,
    refetchInterval: 30_000,
  })
  const { data: incidents = [] } = useQuery({
    queryKey: ['uptime', 'incidents'],
    queryFn: uptimeApi.incidents,
    refetchInterval: 60_000,
  })

  // Cada verificación llega por WebSocket: se recargan los datos (agrupando ráfagas)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => {
    const unsub = ws.subscribe(e => {
      if (e.type !== 'target_check' || timer.current) return
      timer.current = setTimeout(() => {
        timer.current = null
        qc.invalidateQueries({ queryKey: ['uptime'] })
      }, 2000)
    })
    return () => {
      unsub()
      if (timer.current) clearTimeout(timer.current)
    }
  }, [qc])

  const active = targets.filter(t => t.is_active)
  const byId = useMemo(() => new Map(targets.map(t => [t.id, t])), [targets])
  const color = useMemo(() => new Map(active.map((t, i) => [t.id, PALETTE[i % PALETTE.length]])), [active])
  const count = (s: string) => active.filter(t => t.last_status === s).length
  const offline = count('offline')

  if (isLoading) return <div className="p-4 md:p-6 text-muted-foreground">Cargando…</div>

  return (
    <div className="p-4 md:p-6 space-y-4 md:space-y-6">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div>
          <h1 className="text-xl font-bold flex items-center gap-2">
            <HeartPulse className="h-5 w-5 text-primary" />
            Disponibilidad
          </h1>
          <p className="text-sm text-muted-foreground mt-0.5">URLs e IPs verificadas desde el central</p>
        </div>
        <Link to="/uptime/targets">
          <Button size="sm" variant="outline">
            <Settings2 className="h-4 w-4 mr-2" />
            URLs monitoreadas
          </Button>
        </Link>
      </div>

      {active.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center text-muted-foreground">
            <Globe className="h-10 w-10 mx-auto mb-3 opacity-30" />
            <p>No hay URLs activas para monitorear.</p>
            <p className="text-sm mt-1">
              Regístralas en <Link to="/uptime/targets" className="text-primary underline">URLs monitoreadas</Link>.
            </p>
          </CardContent>
        </Card>
      ) : (
        <>
          <VitalSigns
            alert={offline > 0}
            label={offline > 0 ? `${offline} servicio${offline > 1 ? 's' : ''} fuera de línea` : 'Todos los servicios responden'}
          />

          <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 md:gap-4">
            <Kpi label="Total servicios" value={active.length} hint="Monitoreados" icon={Globe} color="text-blue-400" />
            <Kpi label="En línea" value={count('online')} hint="Operativos" icon={CheckCircle2} color="text-green-400" />
            <Kpi label="Fuera de línea" value={offline} hint="Caídos" icon={XCircle} color="text-red-400" />
            <Kpi label="Advertencias" value={count('warning')} hint="Fallando, bajo el umbral" icon={AlertTriangle} color="text-yellow-400" />
          </div>

          <StatusTable targets={active} />

          <ChartCard title="Disponibilidad por servicio (últimos 7 días)">
            <AvailabilityHeatmap targets={active} rows={availability} />
          </ChartCard>

          <div className="grid lg:grid-cols-2 gap-4">
            <ChartCard title="Tiempo de respuesta promedio (24 h)">
              <AvgResponseChart targets={active} rows={times?.avg_24h ?? []} color={color} />
            </ChartCard>
            <ChartCard title="Tiempo de respuesta (última hora)">
              <LastHourChart targets={active} rows={times?.last_hour ?? []} color={color} />
            </ChartCard>
          </div>

          <ChartCard title="Incidentes (últimos 7 días)">
            <IncidentsChart byId={byId} rows={incidents} color={color} />
          </ChartCard>
        </>
      )}
    </div>
  )
}

function StatusTable({ targets }: { targets: Target[] }) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm">Estado de los servicios</CardTitle>
      </CardHeader>
      <CardContent className="overflow-x-auto">
        <table className="w-full text-sm min-w-[640px]">
          <thead>
            <tr className="text-left text-xs text-muted-foreground border-b border-border">
              <th className="py-2 pr-3 font-medium">Estado</th>
              <th className="py-2 pr-3 font-medium">Servicio</th>
              <th className="py-2 pr-3 font-medium">URL / host</th>
              <th className="py-2 pr-3 font-medium text-right">T. respuesta</th>
              <th className="py-2 pr-3 font-medium text-right">Fallos</th>
              <th className="py-2 font-medium">Última verificación</th>
            </tr>
          </thead>
          <tbody>
            {targets.map(t => {
              const s = targetStatus(t)
              return (
                <tr key={t.id} className="border-b border-border/50 last:border-0">
                  <td className="py-2 pr-3"><Badge variant={statusBadge[s]}>{statusLabel[s]}</Badge></td>
                  <td className="py-2 pr-3 font-medium">{t.name}</td>
                  <td className="py-2 pr-3 max-w-[18rem] truncate">
                    {t.monitor_type === 'http' ? (
                      <a href={t.url} target="_blank" rel="noreferrer" className="text-primary hover:underline">{t.url}</a>
                    ) : (
                      <span>{t.url} <span className="text-xs text-muted-foreground">(ping)</span></span>
                    )}
                  </td>
                  <td className="py-2 pr-3 text-right">{t.last_response_ms != null ? `${t.last_response_ms.toFixed(0)} ms` : 'N/A'}</td>
                  <td className="py-2 pr-3 text-right">{t.consecutive_failures}</td>
                  <td className="py-2 text-muted-foreground" title={t.last_check_at ? new Date(t.last_check_at).toLocaleString() : undefined}>
                    {t.last_check_at ? timeAgo(t.last_check_at) : 'Nunca'}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </CardContent>
    </Card>
  )
}

function AvailabilityHeatmap({ targets, rows }: { targets: Target[]; rows: { target_id: string; day: string; total: number; success: number }[] }) {
  const days = lastSevenDays()
  const index = new Map(rows.map(r => [`${r.target_id}|${r.day}`, r]))
  // Una fila por servicio (el primero arriba)
  const names = targets.map(t => t.name).reverse()
  const ordered = [...targets].reverse()
  const data: { value: [number, number, number | null]; total: number }[] = []
  ordered.forEach((t, y) =>
    days.forEach((d, x) => {
      const r = index.get(`${t.id}|${d}`)
      data.push({ value: [x, y, r && r.total > 0 ? Math.round((r.success / r.total) * 1000) / 10 : null], total: r?.total ?? 0 })
    }),
  )

  const option = {
    backgroundColor: 'transparent',
    animation: false,
    tooltip: {
      ...TOOLTIP,
      formatter: (p: any) => {
        const [x, y, v] = p.data.value
        const head = `${esc(names[y])}<br/>${dayLabel(days[x])}`
        return v == null ? `${head}<br/>Sin datos` : `${head}<br/>Disponibilidad: <b>${v}%</b><br/>Verificaciones: ${p.data.total}`
      },
    },
    grid: { left: 8, right: 8, top: 8, bottom: 48, containLabel: true },
    xAxis: { type: 'category', data: days.map(dayLabel), axisLabel: AXIS, axisTick: { show: false }, axisLine: { show: false } },
    yAxis: { type: 'category', data: names, axisLabel: { ...AXIS, width: 160, overflow: 'truncate' }, axisTick: { show: false }, axisLine: { show: false } },
    visualMap: {
      min: 0,
      max: 100,
      calculable: false,
      orient: 'horizontal',
      left: 'center',
      bottom: 0,
      itemHeight: 120,
      textStyle: { color: '#94a3b8', fontSize: 10 },
      formatter: (v: number) => `${v.toFixed(0)}%`,
      inRange: { color: ['#EE6677', '#CCBB44', '#66CCEE', '#228833'] },
      outOfRange: { color: '#1e293b' },
    },
    series: [{
      type: 'heatmap',
      data: data.map(d => ({ ...d, value: [d.value[0], d.value[1], d.value[2] ?? '-'] })),
      label: { show: true, color: '#0f172a', fontSize: 10, formatter: (p: any) => (p.value[2] === '-' ? '' : `${p.value[2]}%`) },
      itemStyle: { borderColor: 'hsl(222 47% 14%)', borderWidth: 2 },
    }],
  }

  return (
    <ReactECharts
      option={option}
      notMerge
      style={{ height: Math.max(180, targets.length * 34 + 80), width: '100%' }}
      opts={{ renderer: 'canvas' }}
    />
  )
}

function AvgResponseChart({ targets, rows, color }: {
  targets: Target[]
  rows: { target_id: string; avg_ms: number | null }[]
  color: Map<string, string>
}) {
  const avg = new Map(rows.map(r => [r.target_id, r.avg_ms]))
  const items = targets.filter(t => avg.get(t.id) != null)
  if (items.length === 0) return <Empty text="Sin verificaciones exitosas en las últimas 24 h." />

  const option = {
    backgroundColor: 'transparent',
    animation: false,
    tooltip: { ...TOOLTIP, trigger: 'axis', axisPointer: { type: 'shadow' }, valueFormatter: (v: number) => `${v.toFixed(0)} ms` },
    grid: { left: 8, right: 16, top: 12, bottom: 8, containLabel: true },
    xAxis: { type: 'value', axisLabel: { ...AXIS, formatter: '{value} ms' }, splitLine: { lineStyle: { color: '#1e293b', type: 'dashed' } } },
    yAxis: {
      type: 'category',
      data: items.map(t => t.name),
      inverse: true,
      axisLabel: { ...AXIS, width: 140, overflow: 'truncate' },
      axisTick: { show: false },
    },
    series: [{
      type: 'bar',
      data: items.map(t => ({ value: Math.round(avg.get(t.id)! * 10) / 10, itemStyle: { color: color.get(t.id) } })),
      barMaxWidth: 22,
      label: { show: true, position: 'right', color: '#94a3b8', fontSize: 10, formatter: (p: any) => `${p.value.toFixed(0)} ms` },
    }],
  }
  return <ReactECharts option={option} notMerge style={{ height: Math.max(200, items.length * 30 + 40), width: '100%' }} />
}

function LastHourChart({ targets, rows, color }: {
  targets: Target[]
  rows: { target_id: string; time: string; response_ms: number | null }[]
  color: Map<string, string>
}) {
  const series: HistorySeries[] = targets
    .map(t => {
      const pts = rows.filter(r => r.target_id === t.id)
      return {
        name: t.name,
        color: color.get(t.id),
        data: toHistorySeries(pts, t.check_interval_secs, p => p.response_ms),
      }
    })
    .filter(s => s.data.length > 0)
  if (series.length === 0) return <Empty text="Sin verificaciones en la última hora." />
  return <HistoryChart series={series} yFormatter={v => `${v.toFixed(0)} ms`} height={260} />
}

function IncidentsChart({ byId, rows, color }: {
  byId: Map<string, Target>
  rows: { target_id: string; time: string; status_code: number | null; error_message: string | null }[]
  color: Map<string, string>
}) {
  const known = rows.filter(r => byId.has(r.target_id))
  if (known.length === 0) return <Empty text="No hay incidentes registrados en los últimos 7 días." />

  const ids = [...new Set(known.map(r => r.target_id))]
  const names = ids.map(id => byId.get(id)!.name)
  const option = {
    backgroundColor: 'transparent',
    animation: false,
    tooltip: {
      ...TOOLTIP,
      formatter: (p: any) => {
        const r = p.data.row
        const when = new Date(r.time).toLocaleString([], { dateStyle: 'short', timeStyle: 'medium' })
        const code = r.status_code != null ? `<br/>Código: ${r.status_code}` : ''
        const err = r.error_message ? `<br/><span style="color:#fca5a5">${esc(r.error_message)}</span>` : ''
        return `<div style="max-width:320px;white-space:normal"><b>${esc(byId.get(r.target_id)!.name)}</b><br/>${when}${code}${err}</div>`
      },
    },
    grid: { left: 8, right: 16, top: 12, bottom: 8, containLabel: true },
    xAxis: {
      type: 'time',
      min: Date.now() - 7 * 86_400_000,
      max: Date.now(),
      axisLabel: { ...AXIS, hideOverlap: true, formatter: { day: '{dd}/{MM}', hour: '{HH}:{mm}' } },
      splitLine: { show: false },
    },
    yAxis: { type: 'category', data: names, axisLabel: { ...AXIS, width: 140, overflow: 'truncate' }, axisTick: { show: false } },
    series: [{
      type: 'scatter',
      symbol: 'path://M2,0 L10,8 L18,0 L20,2 L12,10 L20,18 L18,20 L10,12 L2,20 L0,18 L8,10 L0,2 Z',
      symbolSize: 9,
      data: known.map(r => ({
        value: [new Date(r.time).getTime(), ids.indexOf(r.target_id)],
        row: r,
        itemStyle: { color: color.get(r.target_id) ?? '#f87171' },
      })),
    }],
  }
  return <ReactECharts option={option} notMerge style={{ height: Math.max(200, ids.length * 34 + 60), width: '100%' }} />
}
