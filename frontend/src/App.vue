<template>
  <header class="topbar">
    <span>{{ $t('app.name') }}</span>
    <span v-if="!route.meta.public" class="actions">
      <Button
        v-for="item in navItems"
        :key="item.to"
        :icon="item.icon"
        size="small"
        severity="secondary"
        text
        rounded
        :title="$t(item.label)"
        :aria-label="$t(item.label)"
        @click="item.action"
      />
    </span>
  </header>
  <router-view />
  <Toast position="top-right" group="tr" />
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import Toast from 'primevue/toast'
import Button from 'primevue/button'
import { useRoute, useRouter } from 'vue-router'
import { api } from '@/services/api/http'
import { clearSession, getToken, saveSession } from '@/services/session'

const route = useRoute()
const router = useRouter()

const logout = () => {
  clearSession()
  router.push('/login')
}

const navItems = [
  { icon: 'pi pi-images', to: '/browse', label: 'browse.title', action: () => router.push('/browse') },
  { icon: 'pi pi-chart-line', to: '/dashboard', label: 'dashboard.title', action: () => router.push('/dashboard') },
  { icon: 'pi pi-cog', to: '/settings', label: 'settings.title', action: () => router.push('/settings') },
  { icon: 'pi pi-user', to: '/account', label: 'account.title', action: () => router.push('/account') },
  { icon: 'pi pi-sign-out', to: '/logout', label: 'upload.logout', action: logout },
]

// The admin flag in storage can go stale (role changed elsewhere), so refresh
// it once per page load. The server stays the source of truth for authz.
// The interceptor already clears the session and redirects on 401, so there is
// nothing left to do on failure.
onMounted(() => {
  if (!getToken()) return
  api
    .me()
    .then((response) => {
      const token = getToken()
      if (token) saveSession(token, response.data.is_admin)
    })
    .catch(() => {})
})
</script>

<style>
body {
  margin: 0;
  font-family: system-ui, sans-serif;
}
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 20px;
  font-weight: 600;
  font-size: 1.2rem;
  border-bottom: 1px solid var(--p-content-border-color, #ddd);
}
.topbar .actions {
  display: flex;
  gap: 4px;
}
</style>
