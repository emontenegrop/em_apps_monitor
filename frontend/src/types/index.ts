export type RuntimeKind = 'docker' | 'podman'

/** Runtime reportado por el agente, con el error del último ciclo si lo hubo */
export interface RuntimeStatus {
  kind: RuntimeKind
  socket: string
  version: string | null
  rootless: boolean
  error?: string | null
}

export interface Server {
  id: string
  name: string
  tags: string[]
  is_active: boolean
  status: 'online' | 'offline' | 'unknown'
  last_seen_at: string | null
  /** Tiene una credencial vigente (inscrito y no revocado) */
  agent_enrolled: boolean
  enrollment_expires_at: string | null
  agent_version: string | null
  agent_protocol_version: number | null
  hostname: string | null
  os: string | null
  arch: string | null
  runtimes: RuntimeStatus[]
  last_connected_at: string | null
  created_at: string
  updated_at: string
}

export interface CreateServerPayload {
  name: string
  tags?: string[]
}

export interface Container {
  id: string
  server_id: string
  runtime_kind: RuntimeKind
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

/** net_* (y block_* en contenedores) son tasas en bytes/s */
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

/** Período de las gráficas históricas */
export type HistoryRange = '24h' | '3d' | '5d' | '7d'

/** Serie agregada por intervalos de `bucket_secs` */
export interface History<T> {
  bucket_secs: number
  points: T[]
}

export interface HostHistoryPoint {
  server_id: string
  time: string
  cpu_percent: number | null
  memory_used_bytes: number | null
  memory_total_bytes: number | null
  disk_used_bytes: number | null
  disk_total_bytes: number | null
}

export interface ContainerHistoryPoint {
  container_id: string
  name: string
  time: string
  cpu_percent: number | null
  memory_used_bytes: number | null
  /** Lectura + escritura (bytes/s) */
  disk_io_bytes: number | null
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

export interface RuntimeInfo extends DockerInfo {
  runtime_kind: RuntimeKind
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
  | { type: 'containers'; server_id: string; data: LiveContainer[] }
  | { type: 'docker_info'; server_id: string; runtime: RuntimeKind; data: DockerInfo }
  | { type: 'server_status'; server_id: string; status: string }
  | { type: 'target_check'; target_id: string; status: string }

/** Contenedor tal como llega por WebSocket (formato del agente) */
export interface LiveContainer {
  id: string
  short_id: string
  name: string
  image: string
  image_id: string
  status: string
  state: string
  created: number
  command: string | null
  ports: Port[]
  labels: Record<string, string>
  networks: string[]
  mounts: Mount[]
}
