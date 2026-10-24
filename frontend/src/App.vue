<template>
  <TopBar />
  <router-view />
  <Toast position="top-right" group="tr" />
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import Toast from 'primevue/toast'
import TopBar from '@/components/TopBar.vue'
import { api } from '@/services/api/http'
import { getToken, saveSession } from '@/services/session'

// The admin flag in storage can go stale (role changed elsewhere), so refresh
// it once per page load. The server stays the source of truth for authz.
// The interceptor already clears the session and redirects on 401, so there is
// nothing left to do on failure.
onMounted(() => {
  if (!getToken()) return
  api.getPublicElysiaSettings().then((response) => {
    localStorage.setItem('public_settings', JSON.stringify(response.data))
  })
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
  background: var(--p-content-background);
  color: var(--p-text-color);
}
</style>
