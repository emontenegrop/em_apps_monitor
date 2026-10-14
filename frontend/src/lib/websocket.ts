import type { WsEvent } from '@/types'

type EventHandler = (event: WsEvent) => void

export class MonitorWebSocket {
  private ws: WebSocket | null = null
  private handlers: Set<EventHandler> = new Set()
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private alive = true

  // wss:// cuando la UI se sirve por HTTPS (ws:// sería bloqueado como contenido mixto)
  constructor(
    private url: string = `${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}/ws`,
  ) {}

  connect() {
    if (this.ws?.readyState === WebSocket.OPEN) return
    this.ws = new WebSocket(this.url)

    this.ws.onopen = () => {
      console.info('[WS] Connected')
      if (this.reconnectTimer) {
        clearTimeout(this.reconnectTimer)
        this.reconnectTimer = null
      }
    }

    this.ws.onmessage = (e) => {
      try {
        const event: WsEvent = JSON.parse(e.data)
        this.handlers.forEach(h => h(event))
      } catch {
        /* ignore malformed frames */
      }
    }

    this.ws.onclose = () => {
      console.warn('[WS] Disconnected — reconnecting in 3s…')
      if (this.alive) {
        this.reconnectTimer = setTimeout(() => this.connect(), 3000)
      }
    }

    this.ws.onerror = () => {
      this.ws?.close()
    }
  }

  subscribe(handler: EventHandler): () => void {
    this.handlers.add(handler)
    return () => { this.handlers.delete(handler) }
  }

  close() {
    this.alive = false
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer)
    this.ws?.close()
  }
}

export const ws = new MonitorWebSocket()
