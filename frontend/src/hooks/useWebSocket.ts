import { useEffect, useRef, useState, useCallback } from 'react'
import { ws } from '@/lib/websocket'
import type {
  Container,
  ContainerMetrics,
  DockerInfo,
  HostMetrics,
  WsEvent,
} from '@/types'

export function useWebSocket() {
  const [connected, setConnected] = useState(false)

  useEffect(() => {
    ws.connect()
    const unsub = ws.subscribe(() => {
      // any message means we are connected
      setConnected(true)
    })
    return unsub
  }, [])

  return { connected }
}

/** Subscribe to real-time host metrics for a specific server */
export function useRealtimeHostMetrics(serverId: string | undefined) {
  const [metrics, setMetrics] = useState<HostMetrics | null>(null)

  useEffect(() => {
    if (!serverId) return
    ws.connect()
    return ws.subscribe((event) => {
      if (event.type === 'host_metrics' && event.server_id === serverId) {
        setMetrics(event.data)
      }
    })
  }, [serverId])

  return metrics
}

/** Subscribe to real-time container list for a specific server */
export function useRealtimeContainers(serverId: string | undefined) {
  const [containers, setContainers] = useState<Container[]>([])

  useEffect(() => {
    if (!serverId) return
    ws.connect()
    return ws.subscribe((event) => {
      if (event.type === 'containers' && event.server_id === serverId) {
        setContainers(event.data)
      }
    })
  }, [serverId])

  return containers
}

/** Subscribe to real-time container metrics for a specific server */
export function useRealtimeContainerMetrics(
  serverId: string | undefined,
  containerId: string | undefined,
) {
  const [metrics, setMetrics] = useState<ContainerMetrics | null>(null)

  useEffect(() => {
    if (!serverId || !containerId) return
    ws.connect()
    return ws.subscribe((event) => {
      if (event.type === 'container_metrics' && event.server_id === serverId) {
        const found = event.data.find(m => m.container_id === containerId)
        if (found) setMetrics(found)
      }
    })
  }, [serverId, containerId])

  return metrics
}

/** Subscribe to Docker info updates for a specific server */
export function useRealtimeDockerInfo(serverId: string | undefined) {
  const [info, setInfo] = useState<DockerInfo | null>(null)

  useEffect(() => {
    if (!serverId) return
    ws.connect()
    return ws.subscribe((event) => {
      if (event.type === 'docker_info' && event.server_id === serverId) {
        setInfo(event.data)
      }
    })
  }, [serverId])

  return info
}

/** Rolling history buffer for charting (keeps last N samples) */
export function useMetricsHistory<T>(
  serverId: string | undefined,
  selector: (e: WsEvent) => T | null,
  maxPoints = 60,
) {
  const history = useRef<T[]>([])
  const [, forceRender] = useState(0)

  const tick = useCallback(() => forceRender(n => n + 1), [])

  useEffect(() => {
    if (!serverId) return
    ws.connect()
    return ws.subscribe((event) => {
      const value = selector(event)
      if (value !== null) {
        history.current = [...history.current.slice(-(maxPoints - 1)), value]
        tick()
      }
    })
  }, [serverId, selector, maxPoints, tick])

  return history.current
}
