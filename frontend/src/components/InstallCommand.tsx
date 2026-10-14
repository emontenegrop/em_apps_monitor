import { useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { EnrollmentToken } from '@/lib/api'
import { cn } from '@/lib/utils'

function CopyBlock({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)

  async function copy() {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      /* sin permiso de portapapeles: el texto sigue seleccionable */
    }
  }

  return (
    <div className="relative group">
      <pre className="font-mono text-xs bg-[#0d1117] text-slate-200 rounded-md p-3 pr-10 whitespace-pre-wrap break-all select-all">
        {text}
      </pre>
      <Button
        type="button"
        size="icon"
        variant="ghost"
        className="absolute top-1.5 right-1.5 h-7 w-7"
        onClick={copy}
        title="Copiar"
      >
        {copied ? <Check className="h-3.5 w-3.5 text-green-400" /> : <Copy className="h-3.5 w-3.5" />}
      </Button>
    </div>
  )
}

type Tab = 'systemd' | 'container' | 'manual'

const TABS: { id: Tab; label: string }[] = [
  { id: 'systemd', label: 'Linux (systemd)' },
  { id: 'container', label: 'Contenedor' },
  { id: 'manual', label: 'Manual' },
]

function Note({ children }: { children: React.ReactNode }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

/** Instrucciones para instalar el agente en el host con una ficha de inscripción */
export function InstallCommand({ enrollment }: { enrollment: EnrollmentToken }) {
  const [tab, setTab] = useState<Tab>('systemd')
  const expires = new Date(enrollment.expires_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })

  if (!enrollment.install_command || !enrollment.enroll_command) {
    return (
      <p className="text-sm text-destructive">
        El central no tiene configurada la CA para agentes (AGENT_CA_FILE), así que no puede generar el comando
        de instalación. Revisa la configuración del backend.
      </p>
    )
  }

  const { server_url: url, token, ca_fingerprint: fp } = enrollment
  const containerEnroll =
    `docker run --rm -v em-agent:/etc/em-agent em-agent \\\n` +
    `  enroll --url ${url} --token ${token} --ca-fingerprint ${fp}`
  const containerRun =
    `docker run -d --name em-agent --restart unless-stopped --pid host \\\n` +
    `  -e EM_AGENT_HOST_ROOT=/host \\\n` +
    `  -v em-agent:/etc/em-agent -v /:/host:ro \\\n` +
    `  -v /var/run/docker.sock:/var/run/docker.sock \\\n` +
    `  em-agent`

  return (
    <div className="space-y-4 text-sm">
      <div className="flex gap-1 border-b border-border">
        {TABS.map(t => (
          <button
            key={t.id}
            type="button"
            onClick={() => setTab(t.id)}
            className={cn(
              'px-3 py-1.5 text-xs -mb-px border-b-2 transition-colors',
              tab === t.id
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground',
            )}
          >
            {t.label}
          </button>
        ))}
      </div>

      {tab === 'systemd' && (
        <div className="space-y-2">
          <div className="font-medium">Ejecuta en el host (requiere curl y sudo):</div>
          <CopyBlock text={enrollment.install_command} />
          <Note>
            Verifica la CA del central con su huella, descarga el agente comprobando su SHA-256, lo inscribe y lo deja
            como servicio <code>em-agent</code>. Para Podman sin root agrega <code>--run-as-user USUARIO</code>.
          </Note>
        </div>
      )}

      {tab === 'container' && (
        <div className="space-y-3">
          <Note>
            Requiere la imagen <code>em-agent</code> en el host (<code>docker build -f agent/Dockerfile -t em-agent .</code>{' '}
            desde el repositorio). Con Podman, reemplaza <code>docker</code> por <code>podman</code> y monta{' '}
            <code>/run/podman/podman.sock</code> en lugar de <code>/var/run/docker.sock</code>.
          </Note>
          <div className="space-y-1.5">
            <div className="font-medium">1. Inscribir</div>
            <CopyBlock text={containerEnroll} />
          </div>
          <div className="space-y-1.5">
            <div className="font-medium">2. Ejecutar</div>
            <CopyBlock text={containerRun} />
          </div>
        </div>
      )}

      {tab === 'manual' && (
        <div className="space-y-3">
          <Note>
            Con el binario ya instalado en <code>/usr/local/bin/em-agent</code> (descárgalo de{' '}
            <code>{url}/agent/</code> y verifica <code>SHA256SUMS</code>).
          </Note>
          <div className="space-y-1.5">
            <div className="font-medium">1. Inscribir (como root)</div>
            <CopyBlock text={`sudo ${enrollment.enroll_command}`} />
          </div>
          <div className="space-y-1.5">
            <div className="font-medium">2. Ejecutar</div>
            <CopyBlock text="sudo em-agent run" />
          </div>
          <Note>Para diagnosticar: <code>em-agent check</code>.</Note>
        </div>
      )}

      <Note>
        La ficha es de un solo uso y vence a las {expires}. El agente verifica la CA del central con la huella antes
        de enviarla.
      </Note>
    </div>
  )
}
