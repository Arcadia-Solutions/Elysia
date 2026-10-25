<template>
  <div class="login-wrap">
    <div class="login-card">
      <h2>{{ $t('app.name') }}</h2>
      <Message v-if="error" severity="error" size="small">{{ error }}</Message>
      <template v-else-if="registrationToken">
        <Message severity="info" size="small">{{ $t('legal.sso_intro') }}</Message>
        <LegalAcceptance v-model="acceptLegal" :termsOfService="legal?.terms_of_service ?? ''" :privacyPolicy="legal?.privacy_policy ?? ''" />
        <Button :label="$t('legal.continue')" size="small" :loading :disabled="!acceptLegal" @click="register" />
      </template>
      <Message v-else severity="info" size="small">{{ $t('login.sso_working') }}</Message>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import Button from 'primevue/button'
import Message from 'primevue/message'
import LegalAcceptance from '@/components/LegalAcceptance.vue'
import { api } from '@/services/api/http'
import type { LoginResponse, UnauthElysiaSettings } from '@/services/api-schema'
import { saveSession } from '@/services/session'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const error = ref('')
// Set for a new user, who is only created once they accept the legal texts.
const registrationToken = ref('')
const legal = ref<UnauthElysiaSettings>()
const acceptLegal = ref(false)
const loading = ref(false)

const signIn = (login: LoginResponse) => {
  saveSession(login.token, login.user.is_admin)
  api
    .getPublicElysiaSettings()
    .then((settings) => localStorage.setItem('public_settings', JSON.stringify(settings.data)))
    .finally(() => router.replace('/'))
}

const register = () => {
  loading.value = true
  api
    .ssoRegister({ registration_token: registrationToken.value, accept_legal: acceptLegal.value })
    .then((response) => signIn(response.data))
    .catch(() => {
      error.value = t('login.sso_failed')
    })
    .finally(() => {
      loading.value = false
    })
}

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
      const { token, user, registration_token } = response.data
      if (token && user) return signIn({ token, user })
      registrationToken.value = registration_token ?? ''
      api.getElysiaUnauthSettings().then((settings) => {
        legal.value = settings.data
      })
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
