/// <reference types="vite/client" />

// Ships without types; services/toast.ts emits 'add' and 'remove'.
declare module 'primevue/toasteventbus' {
  const ToastEventBus: { emit: (event: 'add' | 'remove', message: import('primevue/toast').ToastMessageOptions) => void }
  export default ToastEventBus
}
