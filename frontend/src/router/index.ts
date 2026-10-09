import { createRouter, createWebHistory } from 'vue-router'
import LoginView from '@/views/LoginView.vue'
import UploadView from '@/views/UploadView.vue'
import UploadedView from '@/views/UploadedView.vue'
import SettingsView from '@/views/SettingsView.vue'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    { path: '/login', name: 'Login', component: LoginView },
    { path: '/', redirect: '/upload' },
    { path: '/upload', name: 'Upload', component: UploadView },
    { path: '/uploaded', name: 'Uploaded', component: UploadedView },
    { path: '/settings', name: 'Settings', component: SettingsView },
  ],
})

router.beforeEach((to) => {
  if (to.path !== '/login' && !localStorage.getItem('token')) {
    return '/login'
  }
})

export default router
