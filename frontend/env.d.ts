/// <reference types="vite/client" />

// Ships without types; showToast emits its 'add' event.
declare module 'primevue/toasteventbus' {
  const ToastEventBus: { emit: (event: 'add', message: import('primevue/toast').ToastMessageOptions) => void }
  export default ToastEventBus
}
