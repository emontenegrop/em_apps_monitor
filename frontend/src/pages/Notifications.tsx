import { useEffect, useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, Mail, Send, Trash2, UserPlus } from 'lucide-react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { apiErrorMessage } from '@/lib/auth'
import { notificationsApi, type SaveSettingsPayload, type SmtpSecurity } from '@/lib/uptime'

const SELECT =
  'h-9 w-full rounded-md border border-input bg-background px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary'

// Valores iniciales del formulario (no se guardan hasta pulsar Guardar)
const EMPTY: Omit<SaveSettingsPayload, 'password'> = {
  enabled: false,
  smtp_host: '',
  smtp_port: 587,
  security: 'starttls',
  username: '',
  from_address: '',
  single_email_bcc: false,
}

const DEFAULT_PORT: Record<SmtpSecurity, number> = { starttls: 587, ssl: 465, none: 25 }

export function Notifications() {
  const qc = useQueryClient()
  const { data, isLoading } = useQuery({ queryKey: ['notifications'], queryFn: notificationsApi.get })

  const [form, setForm] = useState(EMPTY)
  const [password, setPassword] = useState('')
  const [saved, setSaved] = useState(false)
  const [email, setEmail] = useState('')
  const [name, setName] = useState('')

  // Al cargar, el formulario toma la configuración guardada
  useEffect(() => {
    if (data?.settings) {
      const { updated_at: _, ...s } = data.settings
      setForm(s)
    }
  }, [data?.settings])

  const set = <K extends keyof typeof form>(k: K, v: (typeof form)[K]) => {
    setSaved(false)
    setForm(f => ({ ...f, [k]: v }))
  }

  const save = useMutation({
    mutationFn: () => notificationsApi.save({ ...form, password: password || undefined }),
    onSuccess: d => {
      qc.setQueryData(['notifications'], d)
      setPassword('')
      setSaved(true)
    },
  })
  const add = useMutation({
    mutationFn: () => notificationsApi.addRecipient(email.trim(), name.trim() || undefined),
    onSuccess: () => {
      setEmail('')
      setName('')
      qc.invalidateQueries({ queryKey: ['notifications'] })
    },
  })
  const remove = useMutation({
    mutationFn: notificationsApi.removeRecipient,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['notifications'] }),
  })
  const test = useMutation({ mutationFn: notificationsApi.test })

  function onSave(e: FormEvent) {
    e.preventDefault()
    save.mutate()
  }

  if (isLoading || !data) return <div className="p-4 md:p-6 text-muted-foreground">Cargando…</div>

  const recipients = data.recipients
  const configured = !!data.settings

  return (
    <div className="p-4 md:p-6 space-y-4 md:space-y-6 max-w-3xl">
      <h1 className="text-xl font-bold flex items-center gap-2">
        <Mail className="h-5 w-5 text-primary" />
        Notificaciones por correo
      </h1>
      <p className="text-sm text-muted-foreground -mt-2">
        Se envía un correo cuando una URL monitoreada cae (al llegar a su umbral de fallos) y cuando se recupera.
      </p>

      {!data.can_store_password && (
        <div className="flex items-start gap-2 text-sm text-yellow-400 bg-yellow-500/10 border border-yellow-500/30 rounded-md p-3">
          <AlertTriangle className="h-4 w-4 mt-0.5 shrink-0" />
          El backend no tiene <code>SETTINGS_KEY</code>: no se puede guardar la contraseña SMTP. Defínela en el <code>.env</code> del central.
        </div>
      )}

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Servidor de correo (SMTP)</CardTitle>
          <CardDescription>
            {configured ? `Última modificación: ${new Date(data.settings!.updated_at).toLocaleString()}` : 'Sin configurar'}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSave} className="grid gap-4 sm:grid-cols-2">
            <label className="sm:col-span-2 flex items-center gap-2 text-sm cursor-pointer select-none">
              <input type="checkbox" className="h-4 w-4 accent-primary" checked={form.enabled}
                onChange={e => set('enabled', e.target.checked)} />
              Enviar alertas
            </label>
            <div className="grid gap-1.5">
              <Label htmlFor="n-host">Servidor SMTP *</Label>
              <Input id="n-host" required value={form.smtp_host} onChange={e => set('smtp_host', e.target.value)}
                placeholder="smtp.office365.com" />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div className="grid gap-1.5">
                <Label htmlFor="n-sec">Seguridad</Label>
                <select id="n-sec" className={SELECT} value={form.security}
                  onChange={e => {
                    const s = e.target.value as SmtpSecurity
                    set('security', s)
                    set('smtp_port', DEFAULT_PORT[s])
                  }}>
                  <option value="starttls">STARTTLS</option>
                  <option value="ssl">SSL/TLS</option>
                  <option value="none">Ninguna</option>
                </select>
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor="n-port">Puerto *</Label>
                <Input id="n-port" type="number" min={1} max={65535} required value={form.smtp_port}
                  onChange={e => set('smtp_port', Number(e.target.value))} />
              </div>
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="n-user">Usuario</Label>
              <Input id="n-user" autoComplete="off" value={form.username} onChange={e => set('username', e.target.value)}
                placeholder="cuenta@dominio.com" />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="n-pass">Contraseña</Label>
              <Input id="n-pass" type="password" autoComplete="new-password" value={password}
                disabled={!data.can_store_password}
                onChange={e => {
                  setSaved(false)
                  setPassword(e.target.value)
                }}
                placeholder={data.password_set ? '•••••••• (guardada; escribe para reemplazarla)' : ''} />
            </div>
            <div className="grid gap-1.5 sm:col-span-2">
              <Label htmlFor="n-from">Remitente *</Label>
              <Input id="n-from" type="email" required value={form.from_address}
                onChange={e => set('from_address', e.target.value)} placeholder="cuenta@dominio.com" />
            </div>
            <label className="sm:col-span-2 flex items-start gap-2 text-sm cursor-pointer select-none">
              <input type="checkbox" className="h-4 w-4 mt-0.5 accent-primary" checked={form.single_email_bcc}
                onChange={e => set('single_email_bcc', e.target.checked)} />
              <span>
                Enviar un solo correo con todos los destinatarios en copia oculta (CCO)
                <span className="block text-xs text-muted-foreground">Si no se marca, se envía un correo a cada destinatario.</span>
              </span>
            </label>

            <div className="sm:col-span-2 flex items-center justify-end gap-3 flex-wrap">
              {save.isError && (
                <p className="text-xs text-destructive-foreground bg-destructive/20 p-2 rounded mr-auto">
                  {apiErrorMessage(save.error, 'No se pudo guardar')}
                </p>
              )}
              {saved && <span className="text-xs text-green-400 mr-auto">Configuración guardada.</span>}
              <Button type="submit" disabled={save.isPending}>{save.isPending ? 'Guardando…' : 'Guardar'}</Button>
            </div>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Destinatarios ({recipients.length})</CardTitle>
          <CardDescription>Correos que reciben las alertas de caída y recuperación.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <form
            onSubmit={e => {
              e.preventDefault()
              add.mutate()
            }}
            className="grid gap-3 sm:grid-cols-[1fr_1fr_auto] items-end"
          >
            <div className="grid gap-1.5">
              <Label htmlFor="r-email">Correo *</Label>
              <Input id="r-email" type="email" required value={email} onChange={e => setEmail(e.target.value)}
                placeholder="persona@dominio.com" />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="r-name">Nombre</Label>
              <Input id="r-name" value={name} onChange={e => setName(e.target.value)} />
            </div>
            <Button type="submit" disabled={add.isPending}>
              <UserPlus className="h-4 w-4 mr-2" />
              Agregar
            </Button>
          </form>
          {add.isError && <p className="text-xs text-destructive">{apiErrorMessage(add.error, 'No se pudo agregar')}</p>}

          {recipients.length === 0 ? (
            <p className="text-sm text-muted-foreground">No hay destinatarios: las alertas no se enviarán.</p>
          ) : (
            <ul className="divide-y divide-border">
              {recipients.map(r => (
                <li key={r.id} className="flex items-center gap-3 py-2">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm break-all">{r.email}</div>
                    {r.name && <div className="text-xs text-muted-foreground">{r.name}</div>}
                  </div>
                  <Button size="icon" variant="ghost" className="h-8 w-8 hover:text-destructive-foreground"
                    title="Eliminar" disabled={remove.isPending}
                    onClick={() => {
                      if (confirm(`¿Eliminar a ${r.email} de las notificaciones?`)) remove.mutate(r.id)
                    }}>
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Probar</CardTitle>
          <CardDescription>Envía un correo de prueba a los destinatarios con la configuración guardada.</CardDescription>
        </CardHeader>
        <CardContent className="flex items-center gap-3 flex-wrap">
          <Button variant="outline" disabled={test.isPending || !configured || recipients.length === 0}
            onClick={() => test.mutate()}>
            <Send className="h-4 w-4 mr-2" />
            {test.isPending ? 'Enviando…' : 'Enviar correo de prueba'}
          </Button>
          {test.isSuccess && <span className="text-sm text-green-400">Enviado ({test.data.sent} correo{test.data.sent > 1 ? 's' : ''}).</span>}
          {test.isError && <span className="text-sm text-destructive break-all">{apiErrorMessage(test.error, 'No se pudo enviar')}</span>}
        </CardContent>
      </Card>
    </div>
  )
}
