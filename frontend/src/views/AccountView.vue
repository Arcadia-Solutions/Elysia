<template>
  <div class="account-wrap">
    <h2>{{ $t(userId ? 'users.profile' : 'account.title') }}</h2>

    <div v-if="profile" class="field">
      <label>{{ $t('account.username') }}</label>
      <span>{{ profile.username }}</span>
    </div>
    <div v-if="profile" class="field">
      <label>{{ $t('account.email') }}</label>
      <span>{{ profile.email ?? '-' }}</span>
    </div>
    <div v-if="profile" class="field">
      <label>{{ $t('account.provider') }}</label>
      <span>{{ profile.provider === 'sso' ? $t('account.provider_sso') : $t('account.provider_local') }}</span>
    </div>
    <div v-if="profile" class="field">
      <label>{{ $t('account.role') }}</label>
      <span>{{ profile.is_admin ? $t('account.role_admin') : $t('account.role_user') }}</span>
    </div>

    <div v-if="storage" class="field">
      <label>{{ $t('account.storage') }}</label>
      <span>{{
        storage.quota_bytes == null
          ? $t('account.storage_unlimited', { used: formatBytes(storage.used_bytes) })
          : $t('account.storage_of', { used: formatBytes(storage.used_bytes), quota: formatBytes(storage.quota_bytes) })
      }}</span>
    </div>

    <!-- Another user's profile, opened by an admin from the user list. -->
    <template v-if="userId && user">
      <div class="field">
        <label>{{ $t('account.status') }}</label>
        <span>{{ user.banned ? $t('account.banned') : $t('account.active') }}</span>
      </div>
      <div class="field">
        <label>{{ $t('account.uploads_count') }}</label>
        <span>{{ user.uploads }}</span>
      </div>
      <div class="field">
        <label>{{ $t('account.registered_at') }}</label>
        <span>{{ new Date(user.created_at).toLocaleString('sv-SE') }} ({{ timeAgo(new Date(user.created_at)) }})</span>
      </div>
      <div class="field">
        <label>{{ $t('account.last_seen') }}</label>
        <span>{{ user.last_seen ? `${new Date(user.last_seen).toLocaleString('sv-SE')} (${timeAgo(new Date(user.last_seen))})` : '-' }}</span>
      </div>

      <hr />

      <div class="field">
        <label>{{ $t('account.storage_quota') }}</label>
        <Select
          v-model="quotaMode"
          size="small"
          :options="[
            { label: $t('account.storage_quota_default'), value: 'default' },
            { label: $t('account.storage_quota_unlimited'), value: 'unlimited' },
            { label: $t('account.storage_quota_custom'), value: 'custom' },
          ]"
          option-label="label"
          option-value="value"
        />
        <ByteInput v-if="quotaMode === 'custom'" v-model="quotaBytes" />
        <Button
          :label="$t('account.storage_quota_save')"
          size="small"
          class="save-quota"
          :disabled="quotaMode === 'custom' && !quotaBytes"
          @click="saveQuota"
        />
      </div>

      <hr />

      <div class="actions">
        <Button :label="$t('account.uploads')" icon="pi pi-images" size="small" @click="router.push({ name: 'Browse', query: { username: user.username } })" />
        <Button
          :label="$t(user.banned ? 'account.unban' : 'account.ban')"
          :icon="user.banned ? 'pi pi-check' : 'pi pi-ban'"
          size="small"
          :severity="user.banned ? 'secondary' : 'danger'"
          @click="toggleBan"
        />
      </div>
    </template>

    <template v-if="!userId && profile?.provider !== 'sso'">
      <hr />

      <Button :label="$t('account.password_title')" size="small" @click="showDialog = true" />
      <ChangePasswordDialog v-model:visible="showDialog" />
    </template>

    <template v-if="!userId">
      <hr />

      <ApiKeys />
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import Button from 'primevue/button'
import Select from 'primevue/select'
import ByteInput from '@/components/ByteInput.vue'
import ApiKeys from '@/components/account/ApiKeys.vue'
import ChangePasswordDialog from '@/components/account/ChangePasswordDialog.vue'
import { api } from '@/services/api/http'
import { showToast } from '@/services/toast'
import { formatBytes, timeAgo } from '@/services/format'
import type { PublicElysiaSettings, StorageUsage, UserSummary } from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

// Set on /users/:id, where an admin views someone's profile; /account shows the caller's own.
const userId = computed(() => route.params.id as string | undefined)
const profile = ref<{ username: string; email?: string | null; provider: string; is_admin: boolean } | null>(null)
const user = ref<UserSummary | null>(null)
const showDialog = ref(false)
const storage = ref<StorageUsage | null>(null)
// Quota override as edited: null follows the global default, 0 is unlimited.
const quotaMode = ref<'default' | 'unlimited' | 'custom'>('default')
const quotaBytes = ref(0)
const settings: PublicElysiaSettings | null = JSON.parse(localStorage.getItem('public_settings') ?? 'null')

// Switching to a custom quota starts from the global default, not 0 (unlimited).
watch(quotaMode, (mode) => {
  if (mode === 'custom' && !quotaBytes.value) quotaBytes.value = settings?.default_storage_quota_bytes ?? 0
})

const loadUser = (id: string) =>
  api.getUser(id).then((response) => {
    profile.value = user.value = response.data
    storage.value = { used_bytes: response.data.storage_used_bytes, quota_bytes: response.data.effective_storage_quota_bytes }
    const quota = response.data.storage_quota_bytes
    quotaBytes.value = quota ?? 0
    quotaMode.value = quota == null ? 'default' : quota === 0 ? 'unlimited' : 'custom'
  })

watch(
  userId,
  (id) => {
    profile.value = user.value = storage.value = null
    if (id) {
      loadUser(id)
    } else {
      api.me().then((response) => {
        profile.value = response.data
        storage.value = response.data.storage
      })
    }
  },
  { immediate: true },
)

const saveQuota = () => {
  if (!user.value) return
  const storage_quota_bytes = { default: null, unlimited: 0, custom: quotaBytes.value }[quotaMode.value]
  const { id, username } = user.value
  api.setUserStorageQuota(id, { storage_quota_bytes }).then(() => {
    loadUser(id)
    showToast(t('account.storage_quota_saved', { username }), 'success')
  })
}

const toggleBan = () => {
  if (!user.value) return
  const banned = !user.value.banned
  const username = user.value.username
  if (banned && !confirm(t('account.ban_confirm', { username }))) return
  api.setUserBanned(user.value.id, { banned }).then(() => {
    if (user.value) user.value.banned = banned
    showToast(t(banned ? 'account.banned_toast' : 'account.unbanned_toast', { username }), 'success')
  })
}
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
.save-quota {
  align-self: flex-start;
  margin-top: 4px;
}
.actions {
  display: flex;
  gap: 8px;
}
hr {
  width: 100%;
  border: none;
  border-top: 1px solid var(--p-content-border-color, #ddd);
  margin: 4px 0;
}
</style>
