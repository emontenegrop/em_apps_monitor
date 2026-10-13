import { AlertTriangle } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { runtimeLabel } from '@/lib/utils'
import type { RuntimeStatus } from '@/types'

/** Insignias de los runtimes que reportó el agente (Docker / Podman, rootless, error) */
export function RuntimeBadges({ runtimes, showVersion = false }: { runtimes: RuntimeStatus[]; showVersion?: boolean }) {
  if (!runtimes?.length) return null
  return (
    <>
      {runtimes.map(r => (
        <Badge
          key={`${r.kind}-${r.socket}`}
          variant={r.error ? 'warning' : 'outline'}
          className="text-xs gap-1"
          title={r.error ? `${r.socket}: ${r.error}` : r.socket}
        >
          {r.error && <AlertTriangle className="h-3 w-3" />}
          {runtimeLabel(r.kind)}
          {showVersion && r.version && <span className="text-muted-foreground font-normal">{r.version}</span>}
          {r.rootless && <span className="text-muted-foreground font-normal">rootless</span>}
        </Badge>
      ))}
    </>
  )
}
