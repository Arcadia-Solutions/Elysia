import ToastEventBus from 'primevue/toasteventbus'

// Talks to the <Toast> in App.vue directly, so it works from plain modules (the
// axios interceptor), not just inside components.
export function showToast(detail: string, severity: 'success' | 'warn' | 'error', life = 4000): void {
  ToastEventBus.emit('add', { severity, detail, life, closable: true, group: 'tr' })
}
