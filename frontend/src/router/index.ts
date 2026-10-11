import { createRouter, createWebHistory } from 'vue-router'
import LoginView from '@/views/LoginView.vue'
import RegisterView from '@/views/RegisterView.vue'
import UploadView from '@/views/UploadView.vue'
import UploadedView from '@/views/UploadedView.vue'
import SettingsView from '@/views/SettingsView.vue'
import AccountView from '@/views/AccountView.vue'
import ImageView from '@/views/ImageView.vue'
import BrowseView from '@/views/BrowseView.vue'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    { path: '/login', name: 'Login', component: LoginView },
    { path: '/register', name: 'Register', component: RegisterView },
    { path: '/', redirect: '/upload' },
    { path: '/upload', name: 'Upload', component: UploadView },
    { path: '/uploaded', name: 'Uploaded', component: UploadedView },
    { path: '/browse', name: 'Browse', component: BrowseView, meta: { requiresAdmin: true } },
    { path: '/settings', name: 'Settings', component: SettingsView, meta: { requiresAdmin: true } },
    { path: '/account', name: 'Account', component: AccountView },
    { path: '/i/:id', name: 'Image', component: ImageView },
  ],
})

router.beforeEach((to) => {
  // Image viewer pages are public, like the raw images they show.
  const isPublic = to.path === '/login' || to.path === '/register' || to.path.startsWith('/i/')
  if (isPublic) return
  if (!localStorage.getItem('token')) return '/login'
  // Browse and settings are admin-only, like their APIs.
  if (to.meta.requiresAdmin && localStorage.getItem('is_admin') !== '1') return '/upload'
})

export default router
