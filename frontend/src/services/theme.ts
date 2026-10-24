import { ref, watchEffect } from 'vue'

// The user's choice, falling back to the system preference until they pick one.
const stored = localStorage.getItem('dark_mode')
export const darkMode = ref(stored === null ? window.matchMedia('(prefers-color-scheme: dark)').matches : stored === '1')

// PrimeVue switches to its dark palette when this class is on <html>.
watchEffect(() => document.documentElement.classList.toggle('app-dark', darkMode.value))

export function toggleDarkMode(): void {
  darkMode.value = !darkMode.value
  localStorage.setItem('dark_mode', darkMode.value ? '1' : '0')
}
