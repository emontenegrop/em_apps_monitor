import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, Loader2, Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { InstallCommand } from '@/components/InstallCommand'
import { RuntimeBadges } from '@/components/RuntimeBadges'
import { serversApi, type EnrollmentToken } from '@/lib/api'
import { apiErrorMessage } from '@/lib/auth'

type Step = 'form' | 'install'

/** Alta de servidor: nombre → comando de instalación del agente → espera a que se conecte */
export function AddServerDialog() {
  const [open, setOpen] = useState(false)
  const [step, setStep] = useState<Step>('form')
  const [name, setName] = useState('')
  const [tags, setTags] = useState('')
  const [enrollment, setEnrollment] = useState<EnrollmentToken | null>(null)
  const qc = useQueryClient()

  const create = useMutation({
    mutationFn: async () => {
      const server = await serversApi.create({
        name: name.trim(),
        tags: tags.split(',').map(t => t.trim()).filter(Boolean),
      })
      return serversApi.enrollmentToken(server.id)
    },
    onSuccess: token => {
      setEnrollment(token)
      setStep('install')
      qc.invalidateQueries({ queryKey: ['servers'] })
    },
  })

  // Mientras se muestra el comando, se consulta el servidor hasta que el agente se conecte
  const { data: server } = useQuery({
    queryKey: ['server', enrollment?.server_id],
    queryFn: () => serversApi.get(enrollment!.server_id),
    enabled: open && step === 'install' && !!enrollment,
    refetchInterval: q => (q.state.data?.status === 'online' ? false : 2000),
  })
  const connected = server?.status === 'online' && !!server.last_connected_at

  useEffect(() => {
    if (connected) qc.invalidateQueries({ queryKey: ['servers'] })
  }, [connected, qc])

  function reset() {
    setStep('form')
    setName('')
    setTags('')
    setEnrollment(null)
    create.reset()
  }

  return (
    <Dialog
      open={open}
      onOpenChange={o => {
        setOpen(o)
        if (!o) reset()
      }}
    >
      <DialogTrigger asChild>
        <Button size="sm">
          <Plus className="h-4 w-4 mr-2" />
          Agregar Servidor
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{step === 'form' ? 'Nuevo servidor' : `Instalar el agente en "${name}"`}</DialogTitle>
        </DialogHeader>

        {step === 'form' && (
          <form
            className="grid gap-4 mt-2"
            onSubmit={e => {
              e.preventDefault()
              create.mutate()
            }}
          >
            <p className="text-sm text-muted-foreground">
              El servidor se monitorea con un agente que se instala en el host y se conecta a este central por
              HTTPS. Funciona con Docker y con Podman.
            </p>
            <div className="grid gap-1.5">
              <Label htmlFor="name">Nombre *</Label>
              <Input
                id="name"
                placeholder="Producción — API"
                value={name}
                onChange={e => setName(e.target.value)}
                required
                autoFocus
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="tags">Etiquetas</Label>
              <Input
                id="tags"
                placeholder="produccion, api"
                value={tags}
                onChange={e => setTags(e.target.value)}
              />
              <p className="text-xs text-muted-foreground">Separadas por comas.</p>
            </div>

            {create.isError && (
              <p className="text-xs text-destructive-foreground bg-destructive/20 p-2 rounded">
                {apiErrorMessage(create.error, 'No se pudo crear el servidor')}
              </p>
            )}

            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
                Cancelar
              </Button>
              <Button type="submit" disabled={create.isPending || !name.trim()}>
                {create.isPending ? 'Creando…' : 'Continuar'}
              </Button>
            </div>
          </form>
        )}

        {step === 'install' && enrollment && (
          <div className="space-y-5">
            <InstallCommand enrollment={enrollment} />

            <div className="rounded-md border border-border p-3 text-sm">
              {connected ? (
                <div className="flex items-center gap-2 flex-wrap">
                  <CheckCircle2 className="h-4 w-4 text-green-400" />
                  <span>
                    Agente conectado desde <b>{server?.hostname}</b> (v{server?.agent_version})
                  </span>
                  <RuntimeBadges runtimes={server?.runtimes ?? []} showVersion />
                </div>
              ) : (
                <div className="flex items-center gap-2 text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Esperando a que el agente se conecte…
                </div>
              )}
            </div>

            <div className="flex justify-end">
              <Button onClick={() => setOpen(false)} variant={connected ? 'default' : 'outline'}>
                {connected ? 'Listo' : 'Cerrar (terminaré después)'}
              </Button>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
