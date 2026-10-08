import { config } from '@/config'
import { showToast } from '@/services/toast'
import { ElysiaApiFactory } from '@/services/api-schema'
import axios from 'axios'

// No default Content-Type: axios sets application/json for object bodies and
// multipart/form-data (with boundary) for FormData on its own. A hard default
// would clobber the multipart boundary on uploads.
const http = axios.create({
  baseURL: config.api_base_url,
})

http.interceptors.request.use((cfg) => {
  const token = localStorage.getItem('token')
  if (token && !cfg.url?.includes('/auth/login')) {
    cfg.headers.Authorization = `Bearer ${token}`
  }
  // openapi-generator hard-codes `Content-Type: multipart/form-data` (no boundary)
  // for file uploads; drop it so the browser sets it with a boundary.
  if (cfg.data instanceof FormData) {
    cfg.headers.delete('Content-Type')
  }
  return cfg
})

http.interceptors.response.use(
  (response) => response,
  (error) => {
    // A 401 with a stored token means the session expired: drop it and bounce to
    // login. A 401 without one is a failed login attempt, so let it fall through
    // to the toast below.
    if (error.response?.status === 401 && localStorage.getItem('token')) {
      localStorage.removeItem('token')
      window.location.replace('/login')
      return new Promise(() => {})
    }
    showToast('', error.response?.data?.error ?? 'An unexpected error occurred.', 'error', 4000)
    return Promise.reject(error)
  },
)

export default http

// The generated client, bound to our auth-aware axios instance. Every handler
// shares the `elysia` utoipa tag, so the generator emits a single factory and
// views call methods directly (api.login, api.upload).
// basePath is config.api_base_url so request URLs stay relative (vite proxy).
export const api = ElysiaApiFactory(undefined, config.api_base_url, http)
