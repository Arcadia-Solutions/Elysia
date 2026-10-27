import { existsSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import type { HtmlTagDescriptor, Plugin } from 'vite'

// The e2e tests point the proxy at their own backend.
const api = process.env.ELYSIA_API ?? 'http://localhost:8090'

// Links the optional, gitignored public/custom.css and public/custom.js into
// index.html when they exist, after the app's own css and script.
const customAssets = (): Plugin => ({
  name: 'custom-assets',
  transformIndexHtml: {
    order: 'post',
    handler: () => {
      const exists = (file: string) => existsSync(fileURLToPath(new URL(`./public/${file}`, import.meta.url)))
      const tags: HtmlTagDescriptor[] = []
      if (exists('custom.css')) {
        tags.push({ tag: 'link', attrs: { rel: 'stylesheet', href: '/custom.css' }, injectTo: 'head' })
      }
      if (exists('custom.js')) {
        tags.push({ tag: 'script', attrs: { src: '/custom.js', defer: true }, injectTo: 'body' })
      }
      return tags
    },
  },
})

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue(), customAssets()],
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
