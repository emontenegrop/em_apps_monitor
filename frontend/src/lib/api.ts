import axios from 'axios'
import type {
  Container,
  ContainerMetrics,
  CreateServerPayload,
  DiskUsage,
  DockerInfo,
  HostMetrics,
  Server,
} from '@/types'

const http = axios.create({ baseURL: '/api' })

// ── Servers ────────────────────────────────────────────────────────────────
export const serversApi = {
  list: () => http.get<Server[]>('/servers').then(r => r.data),
  get: (id: string) => http.get<Server>(`/servers/${id}`).then(r => r.data),
  create: (payload: CreateServerPayload) =>
    http.post<Server>('/servers', payload).then(r => r.data),
  update: (id: string, payload: Partial<CreateServerPayload & { is_active: boolean }>) =>
    http.put<Server>(`/servers/${id}`, payload).then(r => r.data),
  delete: (id: string) => http.delete(`/servers/${id}`).then(r => r.data),
  ping: (id: string) => http.post<{ status: string; error?: string }>(`/servers/${id}/ping`).then(r => r.data),
}

// ── Containers ─────────────────────────────────────────────────────────────
export const containersApi = {
  list: (serverId: string, state?: string) =>
    http.get<Container[]>(`/servers/${serverId}/containers`, { params: { state } }).then(r => r.data),
  get: (serverId: string, containerId: string) =>
    http.get<Container>(`/servers/${serverId}/containers/${containerId}`).then(r => r.data),
  inspect: (serverId: string, containerId: string) =>
    http.get(`/servers/${serverId}/containers/${containerId}/inspect`).then(r => r.data),
  logs: (serverId: string, containerId: string) =>
    http.get<string[]>(`/servers/${serverId}/containers/${containerId}/logs`).then(r => r.data),
  images: (serverId: string) =>
    http.get(`/servers/${serverId}/images`).then(r => r.data),
  networks: (serverId: string) =>
    http.get(`/servers/${serverId}/networks`).then(r => r.data),
  volumes: (serverId: string) =>
    http.get(`/servers/${serverId}/volumes`).then(r => r.data),
}

// ── Metrics ────────────────────────────────────────────────────────────────
export const metricsApi = {
  host: (serverId: string, from?: string, to?: string) =>
    http
      .get<HostMetrics[]>(`/servers/${serverId}/metrics/host`, { params: { from, to } })
      .then(r => r.data),
  container: (serverId: string, containerId: string, from?: string, to?: string) =>
    http
      .get<ContainerMetrics[]>(`/servers/${serverId}/containers/${containerId}/metrics`, {
        params: { from, to },
      })
      .then(r => r.data),
  dockerInfo: (serverId: string) =>
    http.get<DockerInfo | null>(`/servers/${serverId}/metrics/docker`).then(r => r.data),
  disks: (serverId: string) =>
    http.get<DiskUsage[]>(`/servers/${serverId}/metrics/disks`).then(r => r.data),
}
