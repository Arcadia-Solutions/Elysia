import type { App } from 'vue'
import type { ToastMessageOptions } from 'primevue/toast'

// Holds the PrimeVue toast service so showToast works from plain modules (the
// axios interceptor), not just inside components. Wired once from main.ts.
let toast: { add: (options: ToastMessageOptions) => void } | undefined

export function initToast(app: App): void {
  toast = app.config.globalProperties.$toast
}

export function showToast(detail: string, severity: 'success' | 'error', life = 4000): void {
  toast?.add({ severity, detail, life, closable: true, group: 'tr' })
}
