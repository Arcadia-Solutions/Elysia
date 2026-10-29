<template>
  <template v-if="isAppReady">
    <TopBar />
    <router-view />
  </template>
  <footer v-if="isAppReady && route.meta.auth && (legal?.terms_of_service || legal?.privacy_policy)" class="footer">
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
import { getToken, setAdmin } from '@/services/session'

const route = useRoute()
const legal = ref<UnauthElysiaSettings>()
const isAppReady = ref(!getToken())

// The public settings and admin flag in storage can go stale (changed
// elsewhere), so refresh them once per page load, before rendering: views read
// them from storage when they are set up. The server stays the source of truth
// for authz. The interceptor already clears the session and redirects on 401,
// so failures only need to unblock rendering.
onMounted(() => {
  api.getElysiaUnauthSettings().then((response) => {
    legal.value = response.data
  })
  if (isAppReady.value) return
  Promise.all([
    api.getPublicElysiaSettings().then((response) => {
      localStorage.setItem('public_settings', JSON.stringify(response.data))
    }),
    api.me().then((response) => setAdmin(response.data.is_admin)),
  ])
    .catch(() => {})
    .finally(() => {
      isAppReady.value = true
    })
})
</script>

<style>
body {
  margin: 0;
  font-family: system-ui, sans-serif;
  background: var(--p-content-background);
  color: var(--p-text-color);
}
.card {
  padding: 20px;
  border: 2px solid var(--p-content-border-color);
  border-radius: 10px;
  background: var(--p-content-background);
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
