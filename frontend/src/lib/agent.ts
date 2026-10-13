import type { Server } from '@/types'

export type AgentState =
  | 'pending' // esperando la primera inscripción del agente
  | 'expired' // la ficha venció sin que el agente se inscribiera
  | 'revoked' // la credencial fue revocada; espera una nueva inscripción
  | 'online'
  | 'offline'

/** Estado del agente de un servidor, con un único criterio para toda la UI */
export function agentState(server: Server): AgentState {
  if (!server.last_connected_at) {
    const expired = server.enrollment_expires_at && new Date(server.enrollment_expires_at) < new Date()
    return expired || !server.enrollment_expires_at ? 'expired' : 'pending'
  }
  if (!server.agent_enrolled) return 'revoked'
  return server.status === 'online' ? 'online' : 'offline'
}

export const agentStateLabel: Record<AgentState, string> = {
  pending: 'Esperando agente',
  expired: 'Sin agente',
  revoked: 'Credencial revocada',
  online: 'En línea',
  offline: 'Agente desconectado',
}

export const agentStateBadge: Record<AgentState, 'success' | 'destructive' | 'warning'> = {
  pending: 'warning',
  expired: 'warning',
  revoked: 'destructive',
  online: 'success',
  offline: 'destructive',
}
