import { createApp } from 'vue'
import PrimeVue from 'primevue/config'
import ToastService from 'primevue/toastservice'
import Aura from '@primeuix/themes/aura'
import 'primeicons/primeicons.css'
import App from './App.vue'
import router from './router'
import i18n from './i18n'

const app = createApp(App)

app.use(PrimeVue, { theme: { preset: Aura } })
app.use(router)
app.use(i18n)
app.use(ToastService)

export function showToast(title: string, detail: string, severity: string, life = 4000, closable = true, group = 'tr'): void {
  app.config.globalProperties.$toast.add({
    severity,
    summary: title,
    detail,
    life,
    closable,
    group,
  })
}

app.mount('#app')
