import React from 'react'
import ReactECharts from 'echarts-for-react'

interface Series {
  name: string
  data: number[]
  color?: string
  unit?: string
  areaStyle?: boolean
}

interface MetricsChartProps {
  title?: string
  timestamps: string[]
  series: Series[]
  height?: number | string
  yMax?: number
  yFormatter?: (v: number) => string
}

export function MetricsChart({
  title,
  timestamps,
  series,
  height = 200,
  yMax,
  yFormatter,
}: MetricsChartProps) {
  const option = {
    backgroundColor: 'transparent',
    animation: false,
    title: title
      ? { text: title, textStyle: { color: '#94a3b8', fontSize: 12 }, top: 4, left: 8 }
      : undefined,
    tooltip: {
      trigger: 'axis',
      backgroundColor: 'rgba(15, 23, 42, 0.95)',
      borderColor: '#334155',
      textStyle: { color: '#e2e8f0', fontSize: 11 },
      formatter: (params: any[]) => {
        const ts = params[0]?.axisValue ?? ''
        const lines = params.map((p: any) => {
          const val = yFormatter ? yFormatter(p.value) : p.value?.toFixed(1)
          return `<span style="color:${p.color}">●</span> ${p.seriesName}: <b>${val}</b>`
        })
        return `<div style="font-size:11px">${ts}<br/>${lines.join('<br/>')}</div>`
      },
    },
    legend: {
      show: series.length > 1,
      textStyle: { color: '#64748b', fontSize: 11 },
      bottom: 0,
    },
    grid: { left: 48, right: 12, top: title ? 28 : 8, bottom: series.length > 1 ? 32 : 8 },
    xAxis: {
      type: 'category',
      data: timestamps.map(t => new Date(t).toLocaleTimeString()),
      axisLine: { lineStyle: { color: '#334155' } },
      axisTick: { show: false },
      axisLabel: { color: '#475569', fontSize: 10, interval: 'auto', rotate: 0 },
      splitLine: { show: false },
    },
    yAxis: {
      type: 'value',
      max: yMax,
      min: 0,
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: '#1e293b', type: 'dashed' } },
      axisLabel: {
        color: '#475569',
        fontSize: 10,
        formatter: yFormatter ?? ((v: number) => v.toFixed(0)),
      },
    },
    series: series.map(s => ({
      name: s.name,
      type: 'line',
      smooth: true,
      symbol: 'none',
      lineStyle: { color: s.color ?? '#3b82f6', width: 1.5 },
      itemStyle: { color: s.color ?? '#3b82f6' },
      areaStyle: s.areaStyle !== false
        ? {
            color: {
              type: 'linear',
              x: 0, y: 0, x2: 0, y2: 1,
              colorStops: [
                { offset: 0, color: `${s.color ?? '#3b82f6'}40` },
                { offset: 1, color: `${s.color ?? '#3b82f6'}05` },
              ],
            },
          }
        : undefined,
      data: s.data,
    })),
  }

  return (
    <ReactECharts
      option={option}
      style={{ height, width: '100%' }}
      opts={{ renderer: 'canvas' }}
    />
  )
}
