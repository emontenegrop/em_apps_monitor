import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { serversApi } from '@/lib/api'
import type { CreateServerPayload } from '@/types'

export function AddServerDialog() {
  const [open, setOpen] = useState(false)
  const [form, setForm] = useState<CreateServerPayload>({
    name: '',
    connection_type: 'local',
    host: 'localhost',
    docker_host: '',
    port: 2375,
  })

  const qc = useQueryClient()
  const mutation = useMutation({
    mutationFn: serversApi.create,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['servers'] })
      setOpen(false)
      setForm({ name: '', connection_type: 'local', host: 'localhost', docker_host: '', port: 2375 })
    },
  })

  const set = (k: keyof CreateServerPayload, v: unknown) =>
    setForm(f => ({ ...f, [k]: v }))

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm">
          <Plus className="h-4 w-4 mr-2" />
          Agregar Servidor
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Nuevo Servidor</DialogTitle>
        </DialogHeader>
        <form
          className="grid gap-4 mt-2"
          onSubmit={e => {
            e.preventDefault()
            mutation.mutate(form)
          }}
        >
          <div className="grid gap-1.5">
            <Label htmlFor="name">Nombre *</Label>
            <Input
              id="name"
              placeholder="Producción — API"
              value={form.name}
              onChange={e => set('name', e.target.value)}
              required
            />
          </div>

          <div className="grid gap-1.5">
            <Label htmlFor="conn">Tipo de conexión</Label>
            <select
              id="conn"
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary"
              value={form.connection_type}
              onChange={e => set('connection_type', e.target.value)}
            >
              <option value="local">Local (Unix socket)</option>
              <option value="tcp">Remoto (TCP)</option>
            </select>
          </div>

          {form.connection_type === 'local' && (
            <div className="grid gap-1.5">
              <Label htmlFor="socket">Docker socket</Label>
              <Input
                id="socket"
                placeholder="unix:///var/run/docker.sock"
                value={form.docker_host ?? ''}
                onChange={e => set('docker_host', e.target.value)}
              />
            </div>
          )}

          {form.connection_type === 'tcp' && (
            <>
              <div className="grid gap-1.5">
                <Label htmlFor="host">Host / IP</Label>
                <Input
                  id="host"
                  placeholder="192.168.1.100"
                  value={form.host ?? ''}
                  onChange={e => set('host', e.target.value)}
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor="port">Puerto Docker</Label>
                <Input
                  id="port"
                  type="number"
                  placeholder="2375"
                  value={form.port ?? 2375}
                  onChange={e => set('port', Number(e.target.value))}
                />
              </div>
            </>
          )}

          {mutation.isError && (
            <p className="text-xs text-destructive-foreground bg-destructive/20 p-2 rounded">
              {(mutation.error as Error).message}
            </p>
          )}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
              Cancelar
            </Button>
            <Button type="submit" disabled={mutation.isPending}>
              {mutation.isPending ? 'Guardando…' : 'Guardar'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
