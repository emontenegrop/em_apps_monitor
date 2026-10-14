import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function formatBytes(bytes: number | null | undefined, decimals = 1): string {
  if (bytes == null) return '—'
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(decimals))} ${sizes[i]}`
}

/** Tasa en bytes/s, p. ej. "1.2 MB/s" */
export function formatRate(bytesPerSec: number | null | undefined, decimals = 1): string {
  if (bytesPerSec == null) return '—'
  return `${formatBytes(bytesPerSec, decimals)}/s`
}

export function runtimeLabel(kind: string | null | undefined): string {
  return kind === 'podman' ? 'Podman' : 'Docker'
}

/** Tiempo relativo corto en español: "hace 3 min" */
export function timeAgo(iso: string | null | undefined): string {
  if (!iso) return '—'
  const secs = Math.max(0, Math.round((Date.now() - new Date(iso).getTime()) / 1000))
  if (secs < 60) return `hace ${secs} s`
  if (secs < 3600) return `hace ${Math.floor(secs / 60)} min`
  if (secs < 86400) return `hace ${Math.floor(secs / 3600)} h`
  return `hace ${Math.floor(secs / 86400)} d`
}

export function formatPercent(value: number | null | undefined, decimals = 1): string {
  if (value == null) return '—'
  return `${value.toFixed(decimals)}%`
}

export function formatUptime(seconds: number | null | undefined): string {
  if (seconds == null) return '—'
  const d = Math.floor(seconds / 86400)
  const h = Math.floor((seconds % 86400) / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  if (d > 0) return `${d}d ${h}h ${m}m`
  if (h > 0) return `${h}h ${m}m`
  return `${m}m`
}

export function containerStateColor(state: string | null | undefined): string {
  switch (state?.toLowerCase()) {
    case 'running': return 'text-green-400'
    case 'exited': return 'text-red-400'
    case 'paused': return 'text-yellow-400'
    case 'created': return 'text-blue-400'
    case 'restarting': return 'text-orange-400'
    default: return 'text-gray-400'
  }
}

export function serverStatusColor(status: string): string {
  switch (status) {
    case 'online': return 'bg-green-500'
    case 'offline': return 'bg-red-500'
    default: return 'bg-yellow-500'
  }
}
