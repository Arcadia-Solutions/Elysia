import { showToast } from '@/services/toast'
import { clearSession, getRefreshToken, getToken, saveTokens } from '@/services/session'
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

declare module 'axios' {
  interface AxiosRequestConfig {
    // Set on the replay after a refresh, so a second 401 logs out instead of looping.
    retried?: boolean
  }
}

// One refresh at a time: concurrent 401s all wait on the same rotation.
let refreshing: Promise<void> | null = null

const refreshTokens = (): Promise<void> => {
  const refreshToken = getRefreshToken()
  if (!refreshToken) return Promise.reject(new Error('no refresh token'))
  refreshing ??= api
    .refresh({ refresh_token: refreshToken })
    .then((response) => saveTokens(response.data.token, response.data.refresh_token))
    .catch((error) => {
      // Another tab rotated the token first and stored the new pair: use it.
      if (getRefreshToken() === refreshToken) throw error
    })
    .finally(() => {
      refreshing = null
    })
  return refreshing
}

const logOut = (error: unknown) => {
  clearSession()
  window.location.replace('/login')
  return Promise.reject(error)
}

http.interceptors.response.use(
  (response) => response,
  (error) => {
    // A 401 with a stored token means the access token expired: refresh it and
    // replay the request once, or drop the session and bounce to login when the
    // session is gone. A 401 without one is a failed login attempt, so let it
    // fall through to the form below.
    // A failed refresh is handled by `refreshTokens`.
    const config = error.config
    if (error.response?.status === 401 && getToken() && !config?.url?.includes('/auth/refresh')) {
      if (!config || config.retried) return logOut(error)
      return refreshTokens().then(
        () => http({ ...config, retried: true }),
        () => logOut(error),
      )
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
