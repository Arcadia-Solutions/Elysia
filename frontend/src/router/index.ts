import { createRouter, createWebHistory } from 'vue-router'
import LoginView from '@/views/LoginView.vue'
import SsoCallbackView from '@/views/SsoCallbackView.vue'
import RegisterView from '@/views/RegisterView.vue'
import UploadView from '@/views/UploadView.vue'
import UploadedView from '@/views/UploadedView.vue'
import SettingsView from '@/views/SettingsView.vue'
import AccountView from '@/views/AccountView.vue'
import ImageView from '@/views/ImageView.vue'
import BrowseView from '@/views/BrowseView.vue'
import UsersView from '@/views/UsersView.vue'
import MaintenanceView from '@/views/MaintenanceView.vue'
import { getToken, isAdmin } from '@/services/session'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    { path: '/login', name: 'Login', component: LoginView, meta: { public: true } },
    {
      path: '/auth/sso/callback',
      name: 'SsoCallback',
      component: SsoCallbackView,
      meta: { public: true },
    },
    { path: '/register', name: 'Register', component: RegisterView, meta: { public: true } },
    { path: '/', redirect: '/upload' },
    { path: '/upload', name: 'Upload', component: UploadView },
    { path: '/uploaded', name: 'Uploaded', component: UploadedView },
    { path: '/browse', name: 'Browse', component: BrowseView },
    {
      path: '/dashboard',
      name: 'Dashboard',
      // Lazy: keeps Highcharts out of the main bundle.
      component: () => import('@/views/DashboardView.vue'),
      meta: { requiresAdmin: true },
    },
    { path: '/settings', name: 'Settings', component: SettingsView, meta: { requiresAdmin: true } },
    { path: '/users', name: 'Users', component: UsersView, meta: { requiresAdmin: true } },
    { path: '/maintenance', name: 'Maintenance', component: MaintenanceView, meta: { requiresAdmin: true } },
    { path: '/users/:id', name: 'User', component: AccountView, meta: { requiresAdmin: true } },
    { path: '/account', name: 'Account', component: AccountView },
    { path: '/i/:id', name: 'Image', component: ImageView },
  ],
})

router.beforeEach((to) => {
  if (to.meta.public) return
  if (!getToken()) return '/login'
  if (to.meta.requiresAdmin && !isAdmin()) return '/upload'
})

export default router
