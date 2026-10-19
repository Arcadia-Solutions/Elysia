<template>
  <div class="api-keys">
    <div class="header">
      <h3>{{ $t('api_keys.title') }}</h3>
      <Button :label="$t('api_keys.create')" icon="pi pi-plus" size="small" @click="showDialog = true" />
    </div>
    <span v-if="!apiKeys.length" class="muted">{{ $t('api_keys.empty') }}</span>
    <div v-for="apiKey in apiKeys" :key="apiKey.id" class="api-key" :class="{ inactive: !apiKey.active }">
      <div class="api-key-title">
        <strong>{{ apiKey.name }}</strong>
        <Tag :value="$t(`api_keys.scope_${apiKey.scope}`)" severity="secondary" />
        <Tag v-if="!apiKey.active" :value="$t(apiKey.revoked_at ? 'api_keys.revoked' : 'api_keys.expired')" severity="danger" />
        <Button v-if="apiKey.active" :label="$t('api_keys.revoke')" size="small" severity="danger" text class="revoke" @click="revoke(apiKey)" />
      </div>
      <span class="muted">
        <code>{{ apiKey.token_prefix }}…</code>
        · {{ $t('api_keys.uploads', { count: apiKey.uploads }) }} · {{ $t('api_keys.last_used') }}
        {{ apiKey.last_used_at ? timeAgo(new Date(apiKey.last_used_at)) : $t('api_keys.never') }}
      </span>
      <span class="muted">
        {{ $t('api_keys.created') }} {{ timeAgo(new Date(apiKey.created_at)) }} · {{ $t('api_keys.expires') }}
        {{ apiKey.expires_at ? timeAgo(new Date(apiKey.expires_at)) : $t('api_keys.never') }}
      </span>
    </div>

    <Dialog v-model:visible="showDialog" modal :header="$t('api_keys.create')" :style="{ width: '25rem' }" @hide="resetForm">
      <div v-if="createdToken" class="dialog-fields">
        <Message severity="warn" size="small">{{ $t('api_keys.token_once') }}</Message>
        <CopyLink :label="$t('api_keys.token')" :value="createdToken" />
        <Button :label="$t('api_keys.done')" size="small" @click="showDialog = false" />
      </div>
      <div v-else class="dialog-fields">
        <InputText v-model="name" size="small" :placeholder="$t('api_keys.name_placeholder')" maxlength="64" fluid />
        <SelectButton
          v-model="scope"
          :options="Object.values(ApiKeyScope).map((value) => ({ value, label: $t(`api_keys.scope_${value}`) }))"
          optionLabel="label"
          optionValue="value"
          :allowEmpty="false"
          size="small"
        />
        <small class="muted">{{ $t(`api_keys.scope_${scope}_help`) }}</small>
        <Select
          v-model="expiry"
          :options="[
            ...[7, 30, 90, 365].map((days) => ({ value: days, label: $t('api_keys.expires_in_days', { days }) })),
            { value: null, label: $t('api_keys.never_expires') },
            { value: 'custom', label: $t('api_keys.custom_date') },
          ]"
          optionLabel="label"
          optionValue="value"
          size="small"
          fluid
        />
        <DatePicker v-if="expiry === 'custom'" v-model="customDate" :minDate="tomorrow" :placeholder="$t('api_keys.expiry_date')" size="small" fluid />
        <Button :label="$t('api_keys.create')" size="small" :loading :disabled="!canCreate" @click="create" />
      </div>
    </Dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Button from 'primevue/button'
import DatePicker from 'primevue/datepicker'
import Dialog from 'primevue/dialog'
import InputText from 'primevue/inputtext'
import Message from 'primevue/message'
import Select from 'primevue/select'
import SelectButton from 'primevue/selectbutton'
import Tag from 'primevue/tag'
import CopyLink from '@/components/CopyLink.vue'
import { api } from '@/services/api/http'
import { showToast } from '@/services/toast'
import { timeAgo } from '@/services/format'
import { ApiKeyScope, type ApiKeySummary } from '@/services/api-schema'

const { t } = useI18n()
const dayMilliseconds = 24 * 60 * 60 * 1000

const apiKeys = ref<ApiKeySummary[]>([])
const showDialog = ref(false)
const loading = ref(false)
const name = ref('')
const scope = ref<ApiKeyScope>(ApiKeyScope.Upload)
// Days until expiry, null for never, or 'custom' to pick a date.
const expiry = ref<number | null | 'custom'>(90)
const customDate = ref<Date | null>(null)
const createdToken = ref('')
const tomorrow = new Date(Date.now() + dayMilliseconds)

// ISO expiry sent to the API: null never expires, undefined while no custom date is picked.
// A custom date expires the key as that day starts (local midnight, as the picker returns it).
const expiresAt = computed(() => {
  if (expiry.value === null) return null
  if (expiry.value !== 'custom') return new Date(Date.now() + expiry.value * dayMilliseconds).toISOString()
  return customDate.value?.toISOString()
})
const canCreate = computed(() => name.value.trim() !== '' && expiresAt.value !== undefined)

const load = () => {
  api.listApiKeys().then((response) => {
    apiKeys.value = response.data
  })
}
load()

const resetForm = () => {
  name.value = ''
  scope.value = ApiKeyScope.Upload
  expiry.value = 90
  customDate.value = null
  createdToken.value = ''
}

const create = () => {
  loading.value = true
  api
    .createApiKey({ name: name.value, scope: scope.value, expires_at: expiresAt.value })
    .then((response) => {
      createdToken.value = response.data.token
      apiKeys.value.unshift(response.data.api_key)
    })
    .finally(() => {
      loading.value = false
    })
}

const revoke = (apiKey: ApiKeySummary) => {
  if (!confirm(t('api_keys.revoke_confirm', { name: apiKey.name }))) return
  api.revokeApiKey(apiKey.id).then(() => {
    showToast(t('api_keys.revoked_toast', { name: apiKey.name }), 'success')
    load()
  })
}
</script>

<style scoped>
.api-keys {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.header h3 {
  margin: 0;
}
.api-key {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px;
  border: 1px solid var(--p-content-border-color, #ddd);
  border-radius: 6px;
}
.api-key.inactive {
  opacity: 0.6;
}
.api-key-title {
  display: flex;
  align-items: center;
  gap: 6px;
}
.revoke {
  margin-left: auto;
}
.muted {
  color: var(--p-text-muted-color, #666);
  font-size: 0.8rem;
}
.dialog-fields {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-top: 8px;
}
</style>
