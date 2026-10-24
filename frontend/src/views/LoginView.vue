<template>
  <div class="login-wrap">
    <div class="login-card">
      <h2>{{ $t('app.name') }}</h2>
      <InputText v-model="login" size="small" :placeholder="$t('login.login_placeholder')" fluid @keyup.enter="handleLogin" />
      <Password v-model="password" size="small" :placeholder="$t('login.password_placeholder')" :feedback="false" toggleMask fluid @keyup.enter="handleLogin" />
      <Message v-if="error" severity="error" size="small">{{ error }}</Message>
      <Button :label="$t('login.submit')" size="small" :loading @click="handleLogin" />
      <Button
        v-if="ssoProviderName"
        :label="$t('login.sso_submit', [ssoProviderName])"
        size="small"
        severity="secondary"
        :loading="ssoLoading"
        @click="handleSsoLogin"
      />
      <router-link v-if="allowRegistration" to="/register" class="register-link">{{ $t('login.create_account') }}</router-link>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import InputText from 'primevue/inputtext'
import Password from 'primevue/password'
import Button from 'primevue/button'
import Message from 'primevue/message'
import { api } from '@/services/api/http'
import { saveSession } from '@/services/session'

const { t } = useI18n()
const router = useRouter()
const login = ref('')
const password = ref('')
const loading = ref(false)
const error = ref('')
const allowRegistration = ref(false)
const ssoProviderName = ref<string | null>(null)
const ssoLoading = ref(false)

api.getElysiaUnauthSettings().then((response) => {
  allowRegistration.value = response.data.allow_registration
  ssoProviderName.value = response.data.sso_provider_name ?? null
})

const handleLogin = () => {
  loading.value = true
  error.value = ''
  api
    .login({ login: login.value, password: password.value })
    .then((response) => {
      saveSession(response.data.token, response.data.user.is_admin)
      api
        .getPublicElysiaSettings()
        .then((settings) => localStorage.setItem('public_settings', JSON.stringify(settings.data)))
        .finally(() => router.push('/'))
    })
    .catch((err) => {
      error.value = err?.response?.status === 429 ? err.response.data.error : t('login.failed')
    })
    .finally(() => {
      loading.value = false
    })
}

const handleSsoLogin = () => {
  ssoLoading.value = true
  error.value = ''
  api
    .ssoLogin()
    .then((response) => {
      // the callback page only accepts the state this browser started, so a
      // callback link crafted by someone else cannot log us into their account.
      // The PKCE verifier is sent back with the code.
      sessionStorage.setItem('ssoState', response.data.state)
      sessionStorage.setItem('ssoCodeVerifier', response.data.code_verifier)
      window.location.href = response.data.authorization_url
    })
    .catch(() => {
      error.value = t('login.sso_failed')
    })
    .finally(() => {
      ssoLoading.value = false
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
.register-link {
  text-align: center;
  font-size: 0.875rem;
}
</style>
