import { createRouter, createWebHistory } from 'vue-router'
import LoginView from '@/views/LoginView.vue'
import UploadView from '@/views/UploadView.vue'
import UploadedView from '@/views/UploadedView.vue'
import SettingsView from '@/views/SettingsView.vue'
import ImageView from '@/views/ImageView.vue'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    { path: '/login', name: 'Login', component: LoginView },
    { path: '/', redirect: '/upload' },
    { path: '/upload', name: 'Upload', component: UploadView },
    { path: '/uploaded', name: 'Uploaded', component: UploadedView },
    { path: '/settings', name: 'Settings', component: SettingsView },
    { path: '/i/:id', name: 'Image', component: ImageView },
  ],
})

router.beforeEach((to) => {
  // Image viewer pages are public, like the raw images they show.
  const isPublic = to.path === '/login' || to.path.startsWith('/i/')
  if (!isPublic && !localStorage.getItem('token')) {
    return '/login'
  }
})

export default router
