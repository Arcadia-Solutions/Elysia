import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// The e2e tests point the proxy at their own backend.
const api = process.env.ELYSIA_API ?? 'http://localhost:8090'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  server: {
    // pinned to 5174 so it does not collide with other Vite apps on 5173, and URLs
    // pointing here (e.g. redirect URIs registered elsewhere) stay valid
    port: 5174,
    strictPort: true,
    proxy: {
      '/api': { target: api, changeOrigin: true },
      '/i': {
        target: api,
        changeOrigin: true,
        // `/i/<id>.<ext>` is the raw image (proxy it); `/i/<id>` with no
        // extension is the SPA viewer page, so serve index.html instead.
        bypass: (req) => (/\.[^/]+$/.test(req.url ?? '') ? undefined : '/index.html'),
      },
      '/t': { target: api, changeOrigin: true },
      '/api-docs': { target: api, changeOrigin: true },
    },
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
})
