import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  server: {
    proxy: {
      '/api': { target: 'http://localhost:8080', changeOrigin: true },
      '/i': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        // `/i/<id>.<ext>` is the raw image (proxy it); `/i/<id>` with no
        // extension is the SPA viewer page, so serve index.html instead.
        bypass: (req) => (/\.[^/]+$/.test(req.url ?? '') ? undefined : '/index.html'),
      },
      '/t': { target: 'http://localhost:8080', changeOrigin: true },
      '/api-docs': { target: 'http://localhost:8080', changeOrigin: true },
    },
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
})
