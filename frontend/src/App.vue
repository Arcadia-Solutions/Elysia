<template>
  <header class="topbar">
    <span>{{ $t('app.name') }}</span>
    <Button
      v-if="route.path !== '/login'"
      icon="pi pi-sign-out"
      size="small"
      severity="secondary"
      text
      rounded
      :title="$t('upload.logout')"
      :aria-label="$t('upload.logout')"
      @click="logout"
    />
  </header>
  <router-view />
  <Toast position="top-right" group="tr" />
</template>

<script setup lang="ts">
import Toast from 'primevue/toast'
import Button from 'primevue/button'
import { useRoute, useRouter } from 'vue-router'

const route = useRoute()
const router = useRouter()

const logout = () => {
  localStorage.removeItem('token')
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
</style>
