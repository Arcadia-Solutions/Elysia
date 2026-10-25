import { showToast } from '@/services/toast'
import { clearSession, getToken } from '@/services/session'
import {
  AlbumsApiFactory,
  APIKeysApiFactory,
  AuthApiFactory,
  FixturesApiFactory,
  ImagesApiFactory,
  MaintenanceApiFactory,
  ReportsApiFactory,
  SettingsApiFactory,
  StatsApiFactory,
  UsersApiFactory,
} from '@/services/api-schema'
import axios from 'axios'

// No default Content-Type: axios sets application/json for object bodies and
// multipart/form-data (with boundary) for FormData on its own. A hard default
// would clobber the multipart boundary on uploads.
const http = axios.create()

http.interceptors.request.use((cfg) => {
  const token = getToken()
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
    // to the form below.
    if (error.response?.status === 401 && getToken()) {
      clearSession()
      window.location.replace('/login')
      return Promise.reject(error)
    }
    // Auth forms (login, register, password change) show their own inline
    // errors, so skip the global toast for them.
    if (error.config?.url?.includes('/auth/')) {
      return Promise.reject(error)
    }
    showToast(error.response?.data?.error ?? 'An unexpected error occurred.', 'error')
    return Promise.reject(error)
  },
)

export default http

// The generated client, bound to our auth-aware axios instance. The generator
// emits one factory per utoipa tag; merge them so views call methods directly
// (api.login, api.upload). Operation ids are unique, so nothing collides.
// An empty basePath keeps request URLs relative (vite proxy).
const factoryArgs = [undefined, '', http] as const
export const api = {
  ...AlbumsApiFactory(...factoryArgs),
  ...APIKeysApiFactory(...factoryArgs),
  ...AuthApiFactory(...factoryArgs),
  ...FixturesApiFactory(...factoryArgs),
  ...ImagesApiFactory(...factoryArgs),
  ...MaintenanceApiFactory(...factoryArgs),
  ...ReportsApiFactory(...factoryArgs),
  ...SettingsApiFactory(...factoryArgs),
  ...StatsApiFactory(...factoryArgs),
  ...UsersApiFactory(...factoryArgs),
}
