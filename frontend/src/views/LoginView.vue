<template>
  <div class="login-wrap">
    <div class="login-card">
      <h2>{{ $t('app.name') }}</h2>
      <Password v-model="token" size="small" :placeholder="$t('login.token_placeholder')" :feedback="false" toggleMask fluid @keyup.enter="handleLogin" />
      <Button :label="$t('login.submit')" size="small" :loading @click="handleLogin" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import Password from 'primevue/password'
import Button from 'primevue/button'
import { api } from '@/services/api/http'

const router = useRouter()
const token = ref('')
const loading = ref(false)

const handleLogin = () => {
  loading.value = true
  api
    .login({ token: token.value })
    .then(() => {
      localStorage.setItem('token', token.value)
      router.push('/')
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
</style>
