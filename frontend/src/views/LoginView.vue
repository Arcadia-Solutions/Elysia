<template>
  <div class="login-wrap">
    <div class="login-card">
      <h2>{{ $t('app.name') }}</h2>
      <InputText v-model="login" size="small" :placeholder="$t('login.login_placeholder')" fluid @keyup.enter="handleLogin" />
      <Password v-model="password" size="small" :placeholder="$t('login.password_placeholder')" :feedback="false" toggleMask fluid @keyup.enter="handleLogin" />
      <Message v-if="error" severity="error" size="small">{{ error }}</Message>
      <Button :label="$t('login.submit')" size="small" :loading @click="handleLogin" />
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

const { t } = useI18n()
const router = useRouter()
const login = ref('')
const password = ref('')
const loading = ref(false)
const error = ref('')
const allowRegistration = ref(false)

api.getElysiaUnauthSettings().then((response) => {
  allowRegistration.value = response.data.allow_registration
})

const handleLogin = () => {
  loading.value = true
  error.value = ''
  api
    .login({ login: login.value, password: password.value })
    .then((response) => {
      localStorage.setItem('token', response.data.token)
      localStorage.setItem('is_admin', response.data.user.is_admin ? '1' : '0')
      router.push('/')
    })
    .catch(() => {
      error.value = t('login.failed')
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
}
</style>
