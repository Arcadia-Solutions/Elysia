<template>
  <div class="login-wrap">
    <div class="login-card">
      <h2>{{ $t('register.title') }}</h2>
      <Message v-if="!allowRegistration && !checking" severity="warn" size="small">{{ $t('register.disabled') }}</Message>
      <template v-else>
        <InputText v-model="username" size="small" :placeholder="$t('register.username_placeholder')" fluid @keyup.enter="handleRegister" />
        <InputText v-model="email" size="small" :placeholder="$t('register.email_placeholder')" fluid @keyup.enter="handleRegister" />
        <Password
          v-model="password"
          size="small"
          :placeholder="$t('register.password_placeholder')"
          :feedback="false"
          toggleMask
          fluid
          @keyup.enter="handleRegister"
        />
        <Message v-if="error" severity="error" size="small">{{ error }}</Message>
        <LegalAcceptance v-if="termsOfService || privacyPolicy" v-model="acceptLegal" :termsOfService :privacyPolicy />
        <Button
          :label="$t('register.submit')"
          size="small"
          :loading
          :disabled="(!!termsOfService || !!privacyPolicy) && !acceptLegal"
          @click="handleRegister"
        />
      </template>
      <router-link to="/login" class="register-link">{{ $t('login.back_to_login') }}</router-link>
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
import LegalAcceptance from '@/components/LegalAcceptance.vue'
import { api } from '@/services/api/http'
import { showToast } from '@/services/toast'

const { t } = useI18n()
const router = useRouter()
const username = ref('')
const email = ref('')
const password = ref('')
const loading = ref(false)
const error = ref('')
const checking = ref(true)
const allowRegistration = ref(false)
const termsOfService = ref('')
const privacyPolicy = ref('')
const acceptLegal = ref(false)

api
  .getElysiaUnauthSettings()
  .then((response) => {
    allowRegistration.value = response.data.allow_registration
    termsOfService.value = response.data.terms_of_service
    privacyPolicy.value = response.data.privacy_policy
  })
  .finally(() => {
    checking.value = false
  })

const handleRegister = () => {
  loading.value = true
  error.value = ''
  api
    .register({ username: username.value, email: email.value, password: password.value, accept_legal: acceptLegal.value })
    .then(() => {
      showToast(t('register.success'), 'success')
      router.push('/login')
    })
    .catch((err) => {
      error.value = err?.response?.data?.error ?? t('register.failed')
    })
    .finally(() => {
      loading.value = false
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
  font-weight: bold;
  text-decoration: none;
  color: inherit;
}
</style>
