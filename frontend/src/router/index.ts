import { createRouter, createWebHistory } from 'vue-router'
import LoginView from '@/views/LoginView.vue'
import SsoCallbackView from '@/views/SsoCallbackView.vue'
import RegisterView from '@/views/RegisterView.vue'
import UploadView from '@/views/UploadView.vue'
import UploadedView from '@/views/UploadedView.vue'
import AccountView from '@/views/AccountView.vue'
import ImageView from '@/views/ImageView.vue'
import BrowseView from '@/views/BrowseView.vue'
import AlbumView from '@/views/AlbumView.vue'
import LegalView from '@/views/LegalView.vue'
import { getToken, isAdmin } from '@/services/session'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    { path: '/login', name: 'Login', component: LoginView, meta: { public: true, auth: true } },
    {
      path: '/auth/sso/callback',
      name: 'SsoCallback',
      component: SsoCallbackView,
      meta: { public: true, auth: true },
    },
    { path: '/register', name: 'Register', component: RegisterView, meta: { public: true, auth: true } },
    { path: '/terms', name: 'Terms', component: LegalView, props: { field: 'terms_of_service' }, meta: { public: true } },
    { path: '/privacy', name: 'Privacy', component: LegalView, props: { field: 'privacy_policy' }, meta: { public: true } },
    { path: '/', redirect: '/upload' },
    { path: '/upload', name: 'Upload', component: UploadView },
    { path: '/uploaded', name: 'Uploaded', component: UploadedView },
    { path: '/browse', name: 'Browse', component: BrowseView },
    { path: '/albums/:id', name: 'Album', component: AlbumView },
    // Admin views are lazy: regular users never download them (Dashboard also pulls in Highcharts).
    {
      path: '/dashboard',
      name: 'Dashboard',
      component: () => import('@/views/DashboardView.vue'),
      meta: { requiresAdmin: true },
    },
    { path: '/settings', name: 'Settings', component: () => import('@/views/SettingsView.vue'), meta: { requiresAdmin: true } },
    { path: '/users', name: 'Users', component: () => import('@/views/UsersView.vue'), meta: { requiresAdmin: true } },
    { path: '/reports', name: 'Reports', component: () => import('@/views/ReportsView.vue'), meta: { requiresAdmin: true } },
    { path: '/maintenance', name: 'Maintenance', component: () => import('@/views/MaintenanceView.vue'), meta: { requiresAdmin: true } },
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
