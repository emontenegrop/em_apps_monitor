export interface Server {
  id: string
  name: string
  host: string
  connection_type: 'local' | 'tcp' | 'ssh'
  docker_host: string | null
  port: number
  tls_enabled: boolean
  tags: string[]
  is_active: boolean
  status: 'online' | 'offline' | 'unknown'
  last_seen_at: string | null
  created_at: string
  updated_at: string
}

export interface CreateServerPayload {
  name: string
  host?: string
  connection_type?: 'local' | 'tcp' | 'ssh'
  docker_host?: string
  port?: number
  tls_enabled?: boolean
  tags?: string[]
}

export interface Container {
  id: string
  server_id: string
  short_id: string | null
  name: string | null
  image: string | null
  image_id: string | null
  status: string | null
  state: string | null
  container_created_at: string | null
  ports: Port[]
  labels: Record<string, string>
  networks: string[]
  mounts: Mount[]
  command: string | null
  updated_at: string
}

export interface Port {
  IP?: string
  PrivatePort?: number
  PublicPort?: number
  Type?: string
}

export interface Mount {
  Type?: string
  Source?: string
  Destination?: string
  Mode?: string
  RW?: boolean
}

export interface HostMetrics {
  time: string
  server_id: string
  cpu_usage_percent: number | null
  memory_used_bytes: number | null
  memory_total_bytes: number | null
  memory_usage_percent: number | null
  swap_used_bytes: number | null
  swap_total_bytes: number | null
  net_rx_bytes: number | null
  net_tx_bytes: number | null
  load_avg_1: number | null
  load_avg_5: number | null
  load_avg_15: number | null
  process_count: number | null
  uptime_seconds: number | null
}

export interface ContainerMetrics {
  time: string
  container_id: string
  server_id: string
  cpu_usage_percent: number | null
  memory_used_bytes: number | null
  memory_limit_bytes: number | null
  memory_usage_percent: number | null
  net_rx_bytes: number | null
  net_tx_bytes: number | null
  block_read_bytes: number | null
  block_write_bytes: number | null
  pids: number | null
}

export interface DockerInfo {
  server_id: string
  docker_version: string | null
  api_version: string | null
  os_type: string | null
  arch: string | null
  kernel_version: string | null
  containers_total: number | null
  containers_running: number | null
  containers_paused: number | null
  containers_stopped: number | null
  images_total: number | null
  storage_driver: string | null
  runtime: string | null
  docker_root_dir: string | null
  swarm_active: boolean | null
  updated_at: string
}

export interface DiskUsage {
  name: string
  mount_point: string
  total_bytes: number
  used_bytes: number
  available_bytes: number
  usage_percent: number
  fs_type: string
}

export type WsEvent =
  | { type: 'host_metrics'; server_id: string; data: HostMetrics }
  | { type: 'container_metrics'; server_id: string; data: ContainerMetrics[] }
  | { type: 'containers'; server_id: string; data: Container[] }
  | { type: 'docker_info'; server_id: string; data: DockerInfo }
  | { type: 'server_status'; server_id: string; status: string }
