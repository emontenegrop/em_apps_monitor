import axios from 'axios'

export interface CurrentUser {
  id: string
  username: string
}

// Cliente aparte: estas llamadas no deben disparar la redirección global por 401
const http = axios.create({ baseURL: '/api/auth' })

export const authApi = {
  me: () => http.get<CurrentUser>('/me').then(r => r.data),
  login: (username: string, password: string) =>
    http.post<{ username: string }>('/login', { username, password }).then(r => r.data),
  logout: () => http.post('/logout'),
  changePassword: (current_password: string, new_password: string) =>
    http.post('/password', { current_password, new_password }),
}

/** Mensaje de error legible de una respuesta de la API */
export function apiErrorMessage(err: unknown, fallback = 'Error inesperado'): string {
  if (axios.isAxiosError(err)) {
    const data = err.response?.data
    if (data && typeof data === 'object' && 'error' in data) return String(data.error)
    if (typeof data === 'string' && data) return data
    if (!err.response) return 'No hay conexión con el servidor'
  }
  return fallback
}
