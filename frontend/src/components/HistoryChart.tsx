import ReactECharts from 'echarts-for-react'

/** Punto de una serie: [ms, valor] y un texto opcional para el tooltip */
export interface HistoryPoint {
  value: [number, number | null]
  detail?: string
}

export interface HistorySeries {
  name: string
  data: HistoryPoint[]
  color?: string
  /** Línea punteada sin área (p. ej. la capacidad total) */
  dashed?: boolean
}

interface HistoryChartProps {
  series: HistorySeries[]
  height?: number
  yMax?: number
  yFormatter: (v: number) => string
}

/** Máximo de series listadas en el tooltip (con muchos contenedores se vuelve ilegible) */
const TOOLTIP_MAX = 10

/** Gráfica de líneas sobre un eje de tiempo real (los huecos sin datos se ven como cortes) */
export function HistoryChart({ series, height = 260, yMax, yFormatter }: HistoryChartProps) {
  const option = {
    backgroundColor: 'transparent',
    animation: false,
    tooltip: {
      trigger: 'axis',
      confine: true,
      backgroundColor: 'rgba(15, 23, 42, 0.95)',
      borderColor: '#334155',
      textStyle: { color: '#e2e8f0', fontSize: 11 },
      formatter: (params: any[]) => {
        const ts = params[0]?.value?.[0]
        const rows = params
          .filter(p => p.value?.[1] != null)
          .sort((a, b) => b.value[1] - a.value[1])
        const lines = rows.slice(0, TOOLTIP_MAX).map(p => {
          const detail = p.data?.detail ? ` <span style="color:#94a3b8">(${p.data.detail})</span>` : ''
          return `<span style="color:${p.color}">●</span> ${p.seriesName}: <b>${yFormatter(p.value[1])}</b>${detail}`
        })
        if (rows.length > TOOLTIP_MAX) lines.push(`<span style="color:#94a3b8">+${rows.length - TOOLTIP_MAX} más</span>`)
        const header = ts != null ? new Date(ts).toLocaleString([], { dateStyle: 'short', timeStyle: 'short' }) : ''
        return `<div style="font-size:11px">${header}<br/>${lines.join('<br/>')}</div>`
      },
    },
    legend: {
      type: 'scroll',
      bottom: 0,
      textStyle: { color: '#94a3b8', fontSize: 11 },
      pageIconColor: '#94a3b8',
      pageTextStyle: { color: '#94a3b8' },
    },
    grid: { left: 8, right: 16, top: 12, bottom: 36, containLabel: true },
    xAxis: {
      type: 'time',
      axisLine: { lineStyle: { color: '#334155' } },
      axisTick: { show: false },
      axisLabel: {
        color: '#64748b',
        fontSize: 10,
        hideOverlap: true,
        formatter: { day: '{dd}/{MM}', hour: '{HH}:{mm}', minute: '{HH}:{mm}' },
      },
      splitLine: { show: false },
    },
    yAxis: {
      type: 'value',
      min: 0,
      max: yMax,
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: '#1e293b', type: 'dashed' } },
      axisLabel: { color: '#64748b', fontSize: 10, formatter: yFormatter },
    },
    series: series.map(s => ({
      name: s.name,
      type: 'line',
      showSymbol: false,
      connectNulls: false,
      lineStyle: { color: s.color, width: 1.5, type: s.dashed ? 'dashed' : 'solid' },
      itemStyle: { color: s.color },
      areaStyle: s.dashed || series.length > 3 ? undefined : { opacity: 0.12 },
      data: s.data,
    })),
  }

  return (
    <ReactECharts
      option={option}
      notMerge
      style={{ height, width: '100%' }}
      opts={{ renderer: 'canvas' }}
    />
  )
}

/**
 * Convierte puntos ordenados por tiempo en una serie, cortando la línea donde faltan
 * intervalos (agente desconectado) en vez de unir los extremos.
 */
export function toHistorySeries<T extends { time: string }>(
  points: T[],
  bucketSecs: number,
  value: (p: T) => number | null,
  detail?: (p: T) => string | undefined,
): HistoryPoint[] {
  const out: HistoryPoint[] = []
  const gap = bucketSecs * 1000 * 1.5
  let prev: number | null = null
  for (const p of points) {
    const t = new Date(p.time).getTime()
    if (prev != null && t - prev > gap) out.push({ value: [prev + bucketSecs * 1000, null] })
    out.push({ value: [t, value(p)], detail: detail?.(p) })
    prev = t
  }
  return out
}
