import { useState, type FormEvent } from 'react'
import { useQuery } from '@tanstack/react-query'
import { KeyRound } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { apiErrorMessage, authApi } from '@/lib/auth'

const MIN_LENGTH = 12

export function Account() {
  const { data: user } = useQuery({ queryKey: ['auth', 'me'], queryFn: authApi.me })
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [confirm, setConfirm] = useState('')
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    if (next !== confirm) {
      setMessage({ ok: false, text: 'La confirmación no coincide' })
      return
    }
    setBusy(true)
    setMessage(null)
    try {
      await authApi.changePassword(current, next)
      setCurrent('')
      setNext('')
      setConfirm('')
      setMessage({ ok: true, text: 'Contraseña actualizada. Se cerraron tus otras sesiones.' })
    } catch (err) {
      setMessage({ ok: false, text: apiErrorMessage(err, 'No se pudo cambiar la contraseña') })
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="p-6 space-y-6 max-w-lg">
      <h1 className="text-xl font-bold flex items-center gap-2">
        <KeyRound className="h-5 w-5 text-primary" />
        Mi cuenta
      </h1>
      <Card>
        <CardHeader>
          <CardTitle>Cambiar contraseña</CardTitle>
          <CardDescription>Usuario: {user?.username}</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="current">Contraseña actual</Label>
              <Input id="current" type="password" autoComplete="current-password" required
                value={current} onChange={e => setCurrent(e.target.value)} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="new">Nueva contraseña</Label>
              <Input id="new" type="password" autoComplete="new-password" required minLength={MIN_LENGTH}
                value={next} onChange={e => setNext(e.target.value)} />
              <p className="text-xs text-muted-foreground">Mínimo {MIN_LENGTH} caracteres.</p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="confirm">Confirmar nueva contraseña</Label>
              <Input id="confirm" type="password" autoComplete="new-password" required
                value={confirm} onChange={e => setConfirm(e.target.value)} />
            </div>
            {message && (
              <p className={message.ok ? 'text-sm text-green-400' : 'text-sm text-destructive'}>{message.text}</p>
            )}
            <Button type="submit" disabled={busy}>{busy ? 'Guardando…' : 'Guardar'}</Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
