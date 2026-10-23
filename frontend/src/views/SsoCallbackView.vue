<template>
  <div class="login-wrap">
    <div class="login-card">
      <h2>{{ $t('app.name') }}</h2>
      <Message v-if="error" severity="error" size="small">{{ error }}</Message>
      <Message v-else severity="info" size="small">{{ $t('login.sso_working') }}</Message>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import Message from 'primevue/message'
import { api } from '@/services/api/http'
import { saveSession } from '@/services/session'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const error = ref('')

const code = route.query.code
const state = route.query.state
const expectedState = sessionStorage.getItem('ssoState')
const codeVerifier = sessionStorage.getItem('ssoCodeVerifier')
sessionStorage.removeItem('ssoState')
sessionStorage.removeItem('ssoCodeVerifier')
// the provider reports a refusal through ?error=; a state this browser did not
// start means the callback link came from someone else
if (route.query.error || typeof code !== 'string' || !code || !codeVerifier || typeof state !== 'string' || !state || state !== expectedState) {
  error.value = t('login.sso_failed')
} else {
  api
    .ssoCallback({ code, code_verifier: codeVerifier })
    .then((response) => {
      saveSession(response.data.token, response.data.user.is_admin)
      api
        .getPublicElysiaSettings()
        .then((settings) => localStorage.setItem('public_settings', JSON.stringify(settings.data)))
        .finally(() => router.replace('/'))
    })
    .catch(() => {
      error.value = t('login.sso_failed')
    })
}
</script>

<style scoped>
.login-wrap {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 80vh;
}
.login-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  width: 280px;
}
.login-card h2 {
  text-align: center;
  margin: 0 0 8px;
}
</style>
