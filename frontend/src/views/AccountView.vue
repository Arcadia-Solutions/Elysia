<template>
  <div class="account-wrap">
    <h2>{{ $t('account.title') }}</h2>

    <div v-if="profile" class="field">
      <label>{{ $t('account.username') }}</label>
      <span>{{ profile.username }}</span>
    </div>
    <div v-if="profile" class="field">
      <label>{{ $t('account.email') }}</label>
      <span>{{ profile.email ?? '—' }}</span>
    </div>
    <div v-if="profile" class="field">
      <label>{{ $t('account.provider') }}</label>
      <span>{{ profile.provider === 'sso' ? $t('account.provider_sso') : $t('account.provider_local') }}</span>
    </div>
    <div v-if="profile" class="field">
      <label>{{ $t('account.role') }}</label>
      <span>{{ profile.is_admin ? $t('account.role_admin') : $t('account.role_user') }}</span>
    </div>

    <template v-if="profile?.provider !== 'sso'">
      <hr />

      <Button :label="$t('account.password_title')" size="small" @click="showDialog = true" />
      <ChangePasswordDialog v-model:visible="showDialog" />
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import Button from 'primevue/button'
import ChangePasswordDialog from '@/components/account/ChangePasswordDialog.vue'
import { api } from '@/services/api/http'
import type { ProfileResponse } from '@/services/api-schema'

const profile = ref<ProfileResponse | null>(null)
const showDialog = ref(false)

api.me().then((response) => {
  profile.value = response.data
})
</script>

<style scoped>
.account-wrap {
  max-width: 420px;
  margin: 0 auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.account-wrap h2 {
  margin: 0;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.field label {
  color: var(--p-text-muted-color, #666);
  font-size: 0.875rem;
}
hr {
  width: 100%;
  border: none;
  border-top: 1px solid var(--p-content-border-color, #ddd);
  margin: 4px 0;
}
</style>
