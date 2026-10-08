import { createApp } from 'vue'
import PrimeVue from 'primevue/config'
import ToastService from 'primevue/toastservice'
import Aura from '@primeuix/themes/aura'
import 'primeicons/primeicons.css'
import App from './App.vue'
import router from './router'
import i18n from './i18n'
import { initToast } from '@/services/toast'

const app = createApp(App)

app.use(PrimeVue, { theme: { preset: Aura } })
app.use(router)
app.use(i18n)
app.use(ToastService)

initToast(app)

app.mount('#app')
