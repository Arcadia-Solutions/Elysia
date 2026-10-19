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
import ApiKeys from '@/components/account/ApiKeys.vue'
import ChangePasswordDialog from '@/components/account/ChangePasswordDialog.vue'
import { api } from '@/services/api/http'
import { showToast } from '@/services/toast'
import { timeAgo } from '@/services/format'
import type { UserSummary } from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

// Set on /users/:id, where an admin views someone's profile; /account shows the caller's own.
const userId = computed(() => route.params.id as string | undefined)
const profile = ref<{ username: string; email?: string | null; provider: string; is_admin: boolean } | null>(null)
const user = ref<UserSummary | null>(null)
const showDialog = ref(false)

watch(
  userId,
  (id) => {
    profile.value = user.value = null
    if (id) {
      api.getUser(id).then((response) => {
        profile.value = user.value = response.data
      })
    } else {
      api.me().then((response) => {
        profile.value = response.data
      })
    }
  },
  { immediate: true },
)

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
