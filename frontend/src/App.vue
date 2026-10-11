<template>
  <header class="topbar">
    <span>{{ $t('app.name') }}</span>
    <span v-if="route.path !== '/login' && route.path !== '/register'" class="actions">
      <Button
        icon="pi pi-images"
        size="small"
        severity="secondary"
        text
        rounded
        :title="$t('browse.title')"
        :aria-label="$t('browse.title')"
        @click="router.push('/browse')"
      />
      <Button
        icon="pi pi-cog"
        size="small"
        severity="secondary"
        text
        rounded
        :title="$t('settings.title')"
        :aria-label="$t('settings.title')"
        @click="router.push('/settings')"
      />
      <Button
        icon="pi pi-user"
        size="small"
        severity="secondary"
        text
        rounded
        :title="$t('account.title')"
        :aria-label="$t('account.title')"
        @click="router.push('/account')"
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
  <router-view />
  <Toast position="top-right" group="tr" />
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import Toast from 'primevue/toast'
import Button from 'primevue/button'
import { useRoute, useRouter } from 'vue-router'
import { api } from '@/services/api/http'

const route = useRoute()
const router = useRouter()

// The admin flag in storage can go stale (role changed elsewhere), so refresh
// it once per page load. The server stays the source of truth for authz.
onMounted(() => {
  if (!localStorage.getItem('token')) return
  api
    .me()
    .then((response) => {
      localStorage.setItem('is_admin', response.data.is_admin ? '1' : '0')
    })
    .catch(() => {})
})

const logout = () => {
  localStorage.removeItem('token')
  localStorage.removeItem('is_admin')
  router.push('/login')
}
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
