import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Globe, Pause, Play, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { apiErrorMessage } from '@/lib/auth'
import {
  statusBadge, statusLabel, targetStatus, uptimeApi,
  type CheckRecord, type CreateTargetPayload, type MonitorType, type Target,
} from '@/lib/uptime'
import { timeAgo } from '@/lib/utils'

const SELECT =
  'h-9 w-full rounded-md border border-input bg-background px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary'

interface FormState {
  name: string
  monitor_type: MonitorType
  url: string
  description: string
  check_interval_secs: string
  timeout_secs: string
  alert_threshold: string
  expected_status_codes: string
}

// Valores iniciales del formulario (no se guardan hasta que se registra)
const EMPTY: FormState = {
  name: '',
  monitor_type: 'http',
  url: '',
  description: '',
  check_interval_secs: '60',
  timeout_secs: '10',
  alert_threshold: '1',
  expected_status_codes: '200,201,202,301,302',
}

function checkSummary(r: CheckRecord): string {
  const parts = [r.is_success ? 'Responde' : 'Falló']
  if (r.status_code != null) parts.push(`HTTP ${r.status_code}`)
  if (r.response_ms != null) parts.push(`${r.response_ms.toFixed(0)} ms`)
  if (r.packet_loss != null) parts.push(`pérdida ${r.packet_loss}%`)
  if (r.ssl_expiry_days != null) parts.push(`certificado vence en ${r.ssl_expiry_days} días`)
  if (r.error_message) parts.push(r.error_message)
  return parts.join(' · ')
}

export function UptimeTargets() {
  const qc = useQueryClient()
  const [form, setForm] = useState<FormState>(EMPTY)
  const [lastCheck, setLastCheck] = useState<Record<string, { ok: boolean; text: string }>>({})
  const set = <K extends keyof FormState>(k: K, v: FormState[K]) => setForm(f => ({ ...f, [k]: v }))
  const refresh = () => qc.invalidateQueries({ queryKey: ['uptime'] })

  const { data: targets = [], isLoading } = useQuery({
    queryKey: ['uptime', 'targets'],
    queryFn: uptimeApi.targets,
    refetchInterval: 30_000,
  })

  const create = useMutation({
    mutationFn: (p: CreateTargetPayload) => uptimeApi.create(p),
    onSuccess: () => {
      setForm(EMPTY)
      refresh()
    },
  })
  const remove = useMutation({ mutationFn: uptimeApi.remove, onSuccess: refresh })
  const toggle = useMutation({
    mutationFn: (t: Target) => uptimeApi.setActive(t.id, !t.is_active),
    onSuccess: refresh,
  })
  const check = useMutation({
    mutationFn: (t: Target) => uptimeApi.checkNow(t.id),
    onSuccess: (r, t) => {
      setLastCheck(c => ({ ...c, [t.id]: { ok: r.is_success, text: checkSummary(r) } }))
      refresh()
    },
    onError: (e, t) => setLastCheck(c => ({ ...c, [t.id]: { ok: false, text: apiErrorMessage(e, 'No se pudo verificar') } })),
  })

  function onSubmit(e: FormEvent) {
    e.preventDefault()
    const http = form.monitor_type === 'http'
    create.mutate({
      name: form.name.trim(),
      monitor_type: form.monitor_type,
      url: form.url.trim(),
      description: form.description.trim() || undefined,
      check_interval_secs: Number(form.check_interval_secs),
      timeout_secs: Number(form.timeout_secs),
      alert_threshold: Number(form.alert_threshold),
      expected_status_codes: http ? form.expected_status_codes.trim() : undefined,
    })
  }

  const isHttp = form.monitor_type === 'http'

  return (
    <div className="p-4 md:p-6 space-y-4 md:space-y-6">
      <div className="flex items-center gap-3 flex-wrap">
        <Link to="/uptime">
          <Button variant="ghost" size="icon" className="h-8 w-8">
            <ArrowLeft className="h-4 w-4" />
          </Button>
        </Link>
        <h1 className="text-xl font-bold flex items-center gap-2">
          <Globe className="h-5 w-5 text-primary" />
          URLs monitoreadas
        </h1>
      </div>

      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm flex items-center gap-2"><Plus className="h-4 w-4" /> Registrar URL o IP</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit} className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <div className="grid gap-1.5 sm:col-span-2">
              <Label htmlFor="t-name">Nombre *</Label>
              <Input id="t-name" required maxLength={255} value={form.name} onChange={e => set('name', e.target.value)}
                placeholder="Portal web" />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="t-type">Tipo *</Label>
              <select id="t-type" className={SELECT} value={form.monitor_type}
                onChange={e => set('monitor_type', e.target.value as MonitorType)}>
                <option value="http">HTTP / HTTPS</option>
                <option value="ping">Ping (ICMP)</option>
              </select>
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="t-interval">Intervalo (s)</Label>
              <Input id="t-interval" type="number" min={30} max={3600} required value={form.check_interval_secs}
                onChange={e => set('check_interval_secs', e.target.value)} />
            </div>
            <div className="grid gap-1.5 sm:col-span-2">
              <Label htmlFor="t-url">{isHttp ? 'URL *' : 'Host o IP *'}</Label>
              <Input id="t-url" required maxLength={512} value={form.url} onChange={e => set('url', e.target.value)}
                placeholder={isHttp ? 'https://servicio.dominio.com/' : '172.16.27.31'} />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="t-timeout">Timeout (s)</Label>
              <Input id="t-timeout" type="number" min={1} max={60} required value={form.timeout_secs}
                onChange={e => set('timeout_secs', e.target.value)} />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="t-threshold">Alertar tras N fallos</Label>
              <Input id="t-threshold" type="number" min={1} max={20} required value={form.alert_threshold}
                onChange={e => set('alert_threshold', e.target.value)} />
            </div>
            <div className="grid gap-1.5 sm:col-span-2">
              <Label htmlFor="t-desc">Descripción</Label>
              <Input id="t-desc" value={form.description} onChange={e => set('description', e.target.value)} />
            </div>
            {isHttp && (
              <div className="grid gap-1.5 sm:col-span-2">
                <Label htmlFor="t-codes">Códigos HTTP esperados</Label>
                <Input id="t-codes" required value={form.expected_status_codes}
                  onChange={e => set('expected_status_codes', e.target.value)} />
              </div>
            )}
            <div className="sm:col-span-2 lg:col-span-4 flex items-center justify-end gap-3 flex-wrap">
              {create.isError && (
                <p className="text-xs text-destructive-foreground bg-destructive/20 p-2 rounded mr-auto">
                  {apiErrorMessage(create.error, 'No se pudo registrar')}
                </p>
              )}
              <Button type="submit" disabled={create.isPending}>
                {create.isPending ? 'Registrando…' : 'Registrar'}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      <div>
        <h2 className="text-sm font-semibold text-muted-foreground mb-3 uppercase tracking-wider">
          Registradas ({targets.length})
        </h2>
        {isLoading ? (
          <div className="text-sm text-muted-foreground">Cargando…</div>
        ) : targets.length === 0 ? (
          <div className="text-center py-10 text-muted-foreground text-sm">
            Todavía no hay URLs registradas.
          </div>
        ) : (
          <div className="space-y-2">
            {targets.map(t => {
              const s = targetStatus(t)
              const res = lastCheck[t.id]
              return (
                <Card key={t.id} className={t.is_active ? '' : 'opacity-60'}>
                  <CardContent className="p-3 md:p-4">
                    <div className="flex items-start gap-3 flex-wrap">
                      <div className="min-w-0 flex-1 space-y-1">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="font-semibold">{t.name}</span>
                          {t.is_active
                            ? <Badge variant={statusBadge[s]}>{statusLabel[s]}</Badge>
                            : <Badge variant="outline">Pausada</Badge>}
                          <Badge variant="outline">{t.monitor_type === 'http' ? 'HTTP' : 'Ping'}</Badge>
                        </div>
                        <div className="text-xs text-primary break-all">{t.url}</div>
                        {t.description && <div className="text-xs text-muted-foreground">{t.description}</div>}
                        <div className="text-xs text-muted-foreground">
                          Cada {t.check_interval_secs} s · timeout {t.timeout_secs} s · alerta tras {t.alert_threshold} fallo{t.alert_threshold > 1 ? 's' : ''}
                          {t.monitor_type === 'http' && <> · códigos {t.expected_status_codes}</>}
                          {' · '}última verificación {t.last_check_at ? timeAgo(t.last_check_at) : 'nunca'}
                        </div>
                        {res && (
                          <div className={`text-xs ${res.ok ? 'text-green-400' : 'text-red-400'} break-all`}>{res.text}</div>
                        )}
                      </div>
                      <div className="flex gap-1.5 flex-wrap">
                        <Button size="sm" variant="outline" disabled={check.isPending && check.variables?.id === t.id}
                          onClick={() => check.mutate(t)} title="Verificar ahora">
                          <RefreshCw className={`h-3.5 w-3.5 mr-1 ${check.isPending && check.variables?.id === t.id ? 'animate-spin' : ''}`} />
                          Verificar
                        </Button>
                        <Button size="sm" variant="outline" disabled={toggle.isPending} onClick={() => toggle.mutate(t)}>
                          {t.is_active ? <Pause className="h-3.5 w-3.5 mr-1" /> : <Play className="h-3.5 w-3.5 mr-1" />}
                          {t.is_active ? 'Pausar' : 'Reanudar'}
                        </Button>
                        <Button size="sm" variant="outline" className="hover:text-destructive-foreground"
                          disabled={remove.isPending}
                          onClick={() => {
                            if (confirm(`¿Eliminar "${t.name}"? Se borra también su historial de verificaciones.`)) remove.mutate(t.id)
                          }}>
                          <Trash2 className="h-3.5 w-3.5 mr-1" />
                          Eliminar
                        </Button>
                      </div>
                    </div>
                  </CardContent>
                </Card>
              )
            })}
          </div>
        )}
        {(remove.isError || toggle.isError) && (
          <p className="text-xs text-destructive mt-2">{apiErrorMessage(remove.error ?? toggle.error, 'La operación falló')}</p>
        )}
      </div>
    </div>
  )
}
