<template>
  <header class="topbar">
    <span class="actions">
      <router-link to="/upload" class="brand">{{ $t('app.name') }}</router-link>
      <Button
        :icon="darkMode ? 'pi pi-sun' : 'pi pi-moon'"
        size="small"
        severity="secondary"
        text
        rounded
        :title="$t('app.dark_mode')"
        :aria-label="$t('app.dark_mode')"
        @click="toggleDarkMode"
      />
    </span>
    <span v-if="!route.meta.public || getToken()" class="actions">
      <Button
        v-for="item in navItems"
        :key="item.to"
        as="router-link"
        :to="item.to"
        :icon="item.icon"
        size="small"
        severity="secondary"
        text
        rounded
        :title="$t(item.label)"
        :aria-label="$t(item.label)"
      />
      <Button
        icon="pi pi-sign-out"
        size="small"
        severity="secondary"
        text
        rounded
        :title="$t('upload.logout')"
        :aria-label="$t('upload.logout')"
        @click="logout"
      />
    </span>
  </header>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import Button from 'primevue/button'
import { useRoute, useRouter } from 'vue-router'
import { api } from '@/services/api/http'
import { clearSession, getToken, isAdmin } from '@/services/session'
import { darkMode, toggleDarkMode } from '@/services/theme'

const route = useRoute()
const router = useRouter()

const logout = async () => {
  // Revoke the token server side, but log out locally even if that fails.
  await api.logout().catch(() => {})
  clearSession()
  router.push('/login')
}

const navItems = computed(() =>
  [
    { icon: 'pi pi-images', to: '/browse', label: 'browse.title' },
    { icon: 'pi pi-chart-line', to: '/dashboard', label: 'dashboard.title', admin: true },
    { icon: 'pi pi-users', to: '/users', label: 'users.title', admin: true },
    { icon: 'pi pi-flag', to: '/reports', label: 'reports.title', admin: true },
    { icon: 'pi pi-wrench', to: '/maintenance', label: 'maintenance.title', admin: true },
    { icon: 'pi pi-cog', to: '/settings', label: 'settings.title', admin: true },
    { icon: 'pi pi-user', to: '/account', label: 'account.title' },
  ].filter((item) => !item.admin || isAdmin()),
)
</script>

<style scoped>
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 20px;
  font-weight: 600;
  font-size: 1.2rem;
  border-bottom: 1px solid var(--p-content-border-color, #ddd);
}
.topbar a {
  text-decoration: none;
}
.topbar .brand {
  color: inherit;
}
.topbar .actions {
  display: flex;
  align-items: center;
  gap: 4px;
}
</style>
