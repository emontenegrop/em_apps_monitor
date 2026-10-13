import { useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { EnrollmentToken } from '@/lib/api'

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

/** Pasos para instalar el agente en el host con una ficha de inscripción */
export function InstallCommand({ enrollment }: { enrollment: EnrollmentToken }) {
  const expires = new Date(enrollment.expires_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })

  if (!enrollment.install_command) {
    return (
      <p className="text-sm text-destructive">
        El central no tiene configurada la CA para agentes (AGENT_CA_FILE), así que no puede generar el comando
        de instalación. Revisa la configuración del backend.
      </p>
    )
  }

  return (
    <div className="space-y-4 text-sm">
      <div className="space-y-1.5">
        <div className="font-medium">1. Copia el binario <code>em-agent</code> al host</div>
        <p className="text-xs text-muted-foreground">
          En <code>/usr/local/bin/em-agent</code>, con permisos de ejecución.
        </p>
      </div>

      <div className="space-y-1.5">
        <div className="font-medium">2. Inscribe el host (como root)</div>
        <CopyBlock text={enrollment.install_command} />
        <p className="text-xs text-muted-foreground">
          La ficha es de un solo uso y vence a las {expires}. El agente verifica la CA del central con la huella
          antes de enviarla.
        </p>
      </div>

      <div className="space-y-1.5">
        <div className="font-medium">3. Inicia el agente</div>
        <CopyBlock text="sudo systemctl enable --now em-agent" />
        <p className="text-xs text-muted-foreground">
          Sin systemd, ejecútalo con <code>em-agent run</code>. Para diagnosticar: <code>em-agent check</code>.
        </p>
      </div>
    </div>
  )
}
