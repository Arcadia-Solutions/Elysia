<template>
  <header class="topbar">
    <span>{{ $t('app.name') }}</span>
    <span v-if="!route.meta.public || getToken()" class="actions">
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
</template>

<script setup lang="ts">
import { computed } from 'vue'
import Button from 'primevue/button'
import { useRoute, useRouter } from 'vue-router'
import { clearSession, getToken, isAdmin } from '@/services/session'

const route = useRoute()
const router = useRouter()

const logout = () => {
  clearSession()
  router.push('/login')
}

const navItems = computed(() =>
  [
    { icon: 'pi pi-images', to: '/browse', label: 'browse.title', action: () => router.push('/browse') },
    { icon: 'pi pi-chart-line', to: '/dashboard', label: 'dashboard.title', action: () => router.push('/dashboard'), admin: true },
    { icon: 'pi pi-users', to: '/users', label: 'users.title', action: () => router.push('/users'), admin: true },
    { icon: 'pi pi-cog', to: '/settings', label: 'settings.title', action: () => router.push('/settings'), admin: true },
    { icon: 'pi pi-user', to: '/account', label: 'account.title', action: () => router.push('/account') },
    { icon: 'pi pi-sign-out', to: '/logout', label: 'upload.logout', action: logout },
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
.topbar .actions {
  display: flex;
  gap: 4px;
}
</style>
