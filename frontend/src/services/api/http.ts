import { config } from '@/config'
import { DefaultApiFactory } from '@/services/api-schema'
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
    if (error.response?.status === 401) {
      localStorage.removeItem('token')
      window.location.replace('/login')
    }
    return Promise.reject(error)
  },
)

export default http

// The generated client, bound to our auth-aware axios instance. Views call its
// methods directly (api.login, api.upload) instead of re-wrapping them.
// basePath is config.api_base_url so request URLs stay relative (vite proxy).
export const api = DefaultApiFactory(undefined, config.api_base_url, http)
