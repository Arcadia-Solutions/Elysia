<template>
  <TopBar />
  <router-view />
  <footer v-if="route.meta.auth && (legal?.terms_of_service || legal?.privacy_policy)" class="footer">
    <router-link v-if="legal.terms_of_service" to="/terms">{{ $t('legal.terms_of_service') }}</router-link>
    <router-link v-if="legal.privacy_policy" to="/privacy">{{ $t('legal.privacy_policy') }}</router-link>
  </footer>
  <Toast position="top-right" group="tr" />
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import Toast from 'primevue/toast'
import TopBar from '@/components/TopBar.vue'
import { api } from '@/services/api/http'
import type { UnauthElysiaSettings } from '@/services/api-schema'
import { getToken, saveSession } from '@/services/session'

const route = useRoute()
const legal = ref<UnauthElysiaSettings>()

// The admin flag in storage can go stale (role changed elsewhere), so refresh
// it once per page load. The server stays the source of truth for authz.
// The interceptor already clears the session and redirects on 401, so there is
// nothing left to do on failure.
onMounted(() => {
  api.getElysiaUnauthSettings().then((response) => {
    legal.value = response.data
  })
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
.footer {
  display: flex;
  justify-content: center;
  gap: 16px;
  padding: 16px;
  font-size: 0.875rem;
}
.footer a {
  font-weight: bold;
  text-decoration: none;
  color: inherit;
}
</style>
