import { http } from '@/lib/api'

// ── Tipos ──────────────────────────────────────────────────────────────────

export type MonitorType = 'http' | 'ping'
export type TargetStatus = 'online' | 'warning' | 'offline'

/** URL o host monitoreado */
export interface Target {
  id: string
  name: string
  url: string
  description: string | null
  monitor_type: MonitorType
  is_active: boolean
  check_interval_secs: number
  timeout_secs: number
  expected_status_codes: string
  alert_threshold: number
  consecutive_failures: number
  last_status: TargetStatus | null
  last_check_at: string | null
  last_response_ms: number | null
  created_at: string
  updated_at: string
}

export interface CreateTargetPayload {
  name: string
  url: string
  description?: string
  monitor_type: MonitorType
  check_interval_secs?: number
  timeout_secs?: number
  expected_status_codes?: string
  alert_threshold?: number
}

export interface CheckRecord {
  target_id: string
  time: string
  is_success: boolean
  response_ms: number | null
  error_message: string | null
  status_code: number | null
  ssl_valid: boolean | null
  ssl_expiry_days: number | null
  redirect_url: string | null
  packet_loss: number | null
  ttl: number | null
  status: TargetStatus
  consecutive_failures: number
}

export interface DailyAvailability {
  target_id: string
  /** YYYY-MM-DD en la zona horaria pedida */
  day: string
  total: number
  success: number
}

export interface ResponseTimes {
  avg_24h: { target_id: string; avg_ms: number | null }[]
  last_hour: { target_id: string; time: string; response_ms: number | null }[]
}

export interface Incident {
  target_id: string
  time: string
  status_code: number | null
  error_message: string | null
}

export type SmtpSecurity = 'starttls' | 'ssl' | 'none'

export interface SmtpSettings {
  enabled: boolean
  smtp_host: string
  smtp_port: number
  security: SmtpSecurity
  username: string
  from_address: string
  single_email_bcc: boolean
  updated_at: string
}

export interface Recipient {
  id: string
  email: string
  name: string | null
  created_at: string
}

export interface NotificationConfig {
  settings: SmtpSettings | null
  password_set: boolean
  can_store_password: boolean
  recipients: Recipient[]
}

export interface SaveSettingsPayload extends Omit<SmtpSettings, 'updated_at'> {
  /** Vacía o ausente: se conserva la guardada */
  password?: string
}

// ── API ────────────────────────────────────────────────────────────────────

export const uptimeApi = {
  targets: () => http.get<Target[]>('/uptime/targets').then(r => r.data),
  create: (p: CreateTargetPayload) => http.post<Target>('/uptime/targets', p).then(r => r.data),
  remove: (id: string) => http.delete(`/uptime/targets/${id}`),
  setActive: (id: string, is_active: boolean) =>
    http.patch<Target>(`/uptime/targets/${id}/active`, { is_active }).then(r => r.data),
  checkNow: (id: string) => http.post<CheckRecord>(`/uptime/targets/${id}/check`).then(r => r.data),
  availability: () =>
    http
      .get<DailyAvailability[]>('/uptime/availability', {
        params: { tz: Intl.DateTimeFormat().resolvedOptions().timeZone },
      })
      .then(r => r.data),
  responseTimes: () => http.get<ResponseTimes>('/uptime/response-times').then(r => r.data),
  incidents: () => http.get<Incident[]>('/uptime/incidents').then(r => r.data),
}

export const notificationsApi = {
  get: () => http.get<NotificationConfig>('/settings/notifications').then(r => r.data),
  save: (p: SaveSettingsPayload) => http.put<NotificationConfig>('/settings/notifications', p).then(r => r.data),
  addRecipient: (email: string, name?: string) =>
    http.post<Recipient>('/settings/notifications/recipients', { email, name }).then(r => r.data),
  removeRecipient: (id: string) => http.delete(`/settings/notifications/recipients/${id}`),
  test: () => http.post<{ sent: number }>('/settings/notifications/test').then(r => r.data),
}

// ── Presentación ───────────────────────────────────────────────────────────

export const statusLabel: Record<TargetStatus | 'pending', string> = {
  online: 'En línea',
  warning: 'Advertencia',
  offline: 'Fuera de línea',
  pending: 'Sin verificar',
}

export const statusBadge: Record<TargetStatus | 'pending', 'success' | 'warning' | 'destructive' | 'outline'> = {
  online: 'success',
  warning: 'warning',
  offline: 'destructive',
  pending: 'outline',
}

export function targetStatus(t: Target): TargetStatus | 'pending' {
  return t.last_status ?? 'pending'
}
