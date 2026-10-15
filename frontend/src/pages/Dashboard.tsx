import { useQuery } from '@tanstack/react-query'
import { Activity, Server, Box, Cpu } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { ServerCard } from '@/components/ServerCard'
import { AddServerDialog } from '@/components/AddServerDialog'
import { HistoryPanel } from '@/components/HistoryPanel'
import { serversApi } from '@/lib/api'

function StatCard({
  icon: Icon, label, value, color,
}: { icon: React.ElementType; label: string; value: string | number; color: string }) {
  return (
    <Card>
      <CardContent className="flex items-center gap-3 md:gap-4 p-3 md:p-4">
        <div className={`p-2 shrink-0 rounded-md ${color}/20`}>
          <Icon className={`h-5 w-5 ${color.replace('bg-', 'text-')}`} />
        </div>
        <div className="min-w-0">
          <div className="text-xl md:text-2xl font-bold">{value}</div>
          <div className="text-xs text-muted-foreground truncate">{label}</div>
        </div>
      </CardContent>
    </Card>
  )
}

export function Dashboard() {
  const { data: servers = [], isLoading } = useQuery({
    queryKey: ['servers'],
    queryFn: serversApi.list,
    refetchInterval: 30_000,
  })

  const online = servers.filter(s => s.status === 'online').length
  const offline = servers.filter(s => s.status === 'offline').length

  return (
    <div className="p-4 md:p-6 space-y-4 md:space-y-6">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div>
          <h1 className="text-xl font-bold flex items-center gap-2">
            <Activity className="h-5 w-5 text-primary" />
            Dashboard
          </h1>
          <p className="text-sm text-muted-foreground mt-0.5">
            Visión general de la infraestructura
          </p>
        </div>
        <AddServerDialog />
      </div>

      {/* Summary stats */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 md:gap-4">
        <StatCard icon={Server} label="Total servidores" value={servers.length} color="bg-blue-500" />
        <StatCard icon={Activity} label="En línea" value={online} color="bg-green-500" />
        <StatCard icon={Activity} label="Fuera de línea" value={offline} color="bg-red-500" />
        <StatCard icon={Box} label="Desconocido" value={servers.length - online - offline} color="bg-yellow-500" />
      </div>

      {servers.length > 0 && <HistoryPanel servers={servers} />}

      {/* Server grid */}
      <div>
        <h2 className="text-sm font-semibold text-muted-foreground mb-3 uppercase tracking-wider">
          Servidores monitoreados
        </h2>
        {isLoading ? (
          <div className="text-muted-foreground text-sm">Cargando…</div>
        ) : servers.length === 0 ? (
          <div className="text-center py-16 text-muted-foreground">
            <Server className="h-12 w-12 mx-auto mb-3 opacity-30" />
            <p>No hay servidores configurados.</p>
            <p className="text-sm mt-1">Agrega uno con el botón de arriba.</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
            {servers.map(s => (
              <ServerCard key={s.id} server={s} />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
