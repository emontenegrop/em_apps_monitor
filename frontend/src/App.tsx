import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Layout } from '@/components/Layout'
import { RequireAuth } from '@/components/RequireAuth'
import { Login } from '@/pages/Login'
import { Account } from '@/pages/Account'
import { Dashboard } from '@/pages/Dashboard'
import { Servers } from '@/pages/Servers'
import { ServerDetail } from '@/pages/ServerDetail'
import { ContainerDetail } from '@/pages/ContainerDetail'
import { Uptime } from '@/pages/Uptime'
import { UptimeTargets } from '@/pages/UptimeTargets'
import { Notifications } from '@/pages/Notifications'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 10_000, retry: 1 },
  },
})

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<Login />} />
          <Route element={<RequireAuth />}>
            <Route element={<Layout />}>
              <Route path="/" element={<Dashboard />} />
              <Route path="/servers" element={<Servers />} />
              <Route path="/servers/:id" element={<ServerDetail />} />
              <Route path="/servers/:id/containers/:cid" element={<ContainerDetail />} />
              <Route path="/uptime" element={<Uptime />} />
              <Route path="/uptime/targets" element={<UptimeTargets />} />
              <Route path="/settings/notifications" element={<Notifications />} />
              <Route path="/account" element={<Account />} />
            </Route>
          </Route>
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  )
}
