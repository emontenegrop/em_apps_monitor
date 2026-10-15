import { cn } from '@/lib/utils'

/** Un latido del trazo ECG, desplazado `o` unidades */
function beat(o: number) {
  return (
    `M${o},30 L${o + 60},30 Q${o + 70},22 ${o + 80},30 L${o + 95},30 L${o + 100},38 ` +
    `L${o + 108},5 L${o + 116},54 L${o + 122},30 L${o + 140},30 Q${o + 155},17 ${o + 170},30 L${o + 200},30`
  )
}

const TRACE = [0, 200, 400].map(beat).join(' ')

/**
 * "Signos vitales" del monitoreo: trazo ECG continuo, verde si todo responde y rojo
 * (más rápido) si algún servicio está fuera de línea.
 */
export function VitalSigns({ alert, label }: { alert: boolean; label: string }) {
  return (
    <div
      className={cn(
        'vital-signs relative h-16 overflow-hidden rounded-lg border bg-card',
        alert ? 'vital-alert border-red-500/40 text-red-400' : 'border-green-500/30 text-green-400',
      )}
      role="img"
      aria-label={label}
    >
      <div className="vital-grid absolute inset-0" />
      <svg className="absolute inset-0 h-full w-full" viewBox="0 0 600 60" preserveAspectRatio="none" aria-hidden>
        <g className="vital-track">
          <path d={TRACE} fill="none" stroke="currentColor" strokeWidth="2" vectorEffect="non-scaling-stroke" />
          <path
            d={TRACE}
            transform="translate(600 0)"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            vectorEffect="non-scaling-stroke"
          />
        </g>
      </svg>
      <div className="absolute right-3 top-2 flex items-center gap-1.5 text-xs font-semibold">
        <span className={cn('h-2 w-2 rounded-full', alert ? 'bg-red-500 animate-ping' : 'bg-green-400')} />
        {label}
      </div>
    </div>
  )
}
