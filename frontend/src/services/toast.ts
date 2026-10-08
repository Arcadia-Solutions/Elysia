import type { App } from 'vue'

// Holds the PrimeVue toast service so showToast works from plain modules (the
// axios interceptor), not just inside components. Wired once from main.ts.
let toast: { add: (options: Record<string, unknown>) => void } | undefined

export function initToast(app: App): void {
  toast = app.config.globalProperties.$toast
}

export function showToast(title: string, detail: string, severity: string, life = 4000, closable = true, group = 'tr'): void {
  toast?.add({ severity, summary: title, detail, life, closable, group })
}
