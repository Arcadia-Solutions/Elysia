import { toRaw } from 'vue'
import type { ToastMessageOptions } from 'primevue/toast'
import ToastEventBus from 'primevue/toasteventbus'

// PrimeVue can only pause its life timer on hover, not restart it, so toasts
// close through our own timer instead. The progress bar in App.vue assumes this
// duration.
const LIFE = 4000
const timers = new Map<ToastMessageOptions, ReturnType<typeof setTimeout>>()

// Also the <Toast> mouseleave handler, hence the event shape.
export function startToastTimer({ message }: { message: ToastMessageOptions }): void {
  message = toRaw(message)
  stopToastTimer({ message })
  timers.set(
    message,
    setTimeout(() => {
      timers.delete(message)
      ToastEventBus.emit('remove', message)
    }, LIFE),
  )
}

export function stopToastTimer({ message }: { message: ToastMessageOptions }): void {
  clearTimeout(timers.get(toRaw(message)))
}

// Talks to the <Toast> in App.vue directly, so it works from plain modules (the
// axios interceptor), not just inside components.
export function showToast(detail: string, severity: 'success' | 'warn' | 'error'): void {
  const message: ToastMessageOptions = { severity, detail, closable: true, group: 'tr' }
  ToastEventBus.emit('add', message)
  startToastTimer({ message })
}
