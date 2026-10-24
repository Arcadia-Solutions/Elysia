<template>
  <Dialog v-model:visible="visible" modal :header="$t('account.password_title')" :style="{ width: '25rem' }" @hide="resetForm">
    <div class="dialog-fields">
      <Password
        v-model="currentPassword"
        size="small"
        :placeholder="$t('account.current_placeholder')"
        :feedback="false"
        toggleMask
        fluid
        @keyup.enter="handleChange"
      />
      <Password
        v-model="newPassword"
        size="small"
        :placeholder="$t('account.new_placeholder')"
        :feedback="false"
        toggleMask
        fluid
        @keyup.enter="handleChange"
      />
      <Password
        v-model="confirmPassword"
        size="small"
        :placeholder="$t('account.confirm_placeholder')"
        :feedback="false"
        toggleMask
        fluid
        @keyup.enter="handleChange"
      />
      <Message v-if="error" severity="error" size="small">{{ error }}</Message>
      <Button :label="$t('account.submit')" size="small" :loading @click="handleChange" />
    </div>
  </Dialog>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Password from 'primevue/password'
import Button from 'primevue/button'
import Message from 'primevue/message'
import Dialog from 'primevue/dialog'
import { api } from '@/services/api/http'
import { showToast } from '@/services/toast'

const { t } = useI18n()
const visible = defineModel<boolean>('visible', { required: true })
const currentPassword = ref('')
const newPassword = ref('')
const confirmPassword = ref('')
const loading = ref(false)
const error = ref('')

const resetForm = () => {
  currentPassword.value = ''
  newPassword.value = ''
  confirmPassword.value = ''
  error.value = ''
}

const handleChange = () => {
  if (newPassword.value !== confirmPassword.value) {
    error.value = t('account.mismatch')
    return
  }
  loading.value = true
  error.value = ''
  api
    .changePassword({ current_password: currentPassword.value, new_password: newPassword.value })
    .then(() => {
      visible.value = false
      showToast(t('account.success'), 'success')
    })
    .finally(() => {
      loading.value = false
    })
}
</script>

<style scoped>
.dialog-fields {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-top: 8px;
}
</style>
