import { useQuery } from '@tanstack/react-query'
import { Server } from 'lucide-react'
import { ServerCard } from '@/components/ServerCard'
import { AddServerDialog } from '@/components/AddServerDialog'
import { serversApi } from '@/lib/api'

export function Servers() {
  const { data: servers = [], isLoading } = useQuery({
    queryKey: ['servers'],
    queryFn: serversApi.list,
    refetchInterval: 30_000,
  })

  return (
    <div className="p-4 md:p-6 space-y-4 md:space-y-6">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <h1 className="text-xl font-bold flex items-center gap-2">
          <Server className="h-5 w-5 text-primary" />
          Servidores
        </h1>
        <AddServerDialog />
      </div>

      {isLoading ? (
        <div className="text-muted-foreground">Cargando…</div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
          {servers.map(s => (
            <ServerCard key={s.id} server={s} />
          ))}
        </div>
      )}
    </div>
  )
}
