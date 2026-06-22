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
