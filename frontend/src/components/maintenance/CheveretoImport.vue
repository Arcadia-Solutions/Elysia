<template>
  <div class="chevereto-import">
    <small>{{ $t('maintenance.chevereto_import.help') }}</small>

    <h3>{{ $t('maintenance.chevereto_import.chevereto') }}</h3>
    <div class="field">
      <label>{{ $t('maintenance.chevereto_import.chevereto_database_url.label') }}</label>
      <small>{{ $t('maintenance.chevereto_import.chevereto_database_url.help') }}</small>
      <InputText v-model="form.chevereto_database_url" size="small" fluid placeholder="mysql://chevereto:password@localhost:3306/chevereto" :disabled />
    </div>
    <div class="field">
      <label>{{ $t('maintenance.chevereto_import.chevereto_public_url.label') }}</label>
      <small>{{ $t('maintenance.chevereto_import.chevereto_public_url.help') }}</small>
      <InputText v-model="form.chevereto_public_url" size="small" fluid placeholder="https://img.example.com" :disabled />
    </div>
    <div class="field">
      <label>{{ $t('maintenance.chevereto_import.link_hosts.label') }}</label>
      <small>{{ $t('maintenance.chevereto_import.link_hosts.help') }}</small>
      <InputText v-model="linkHosts" size="small" fluid placeholder="img.example.com, old.example.com" :disabled />
    </div>

    <h3>{{ $t('maintenance.chevereto_import.backend') }}</h3>
    <div class="field">
      <label>{{ $t('maintenance.chevereto_import.backend_database_url.label') }}</label>
      <small>{{ $t('maintenance.chevereto_import.backend_database_url.help') }}</small>
      <InputText v-model="form.backend_database_url" size="small" fluid placeholder="postgres://app:password@localhost:5432/app" :disabled />
    </div>
    <div class="field">
      <label>{{ $t('maintenance.chevereto_import.match_users.label') }}</label>
      <small>{{ $t('maintenance.chevereto_import.match_users.help') }}</small>
      <ToggleSwitch v-model="matchUsers" :disabled="disabled || !form.backend_database_url?.trim()" />
    </div>
    <template v-if="matchUsers && form.backend_database_url?.trim()">
      <div class="field">
        <label>{{ $t('maintenance.chevereto_import.users_table.label') }}</label>
        <small>{{ $t('maintenance.chevereto_import.users_table.help') }}</small>
        <InputText v-model="users.table" size="small" fluid :disabled />
      </div>
      <div class="field">
        <label>{{ $t('maintenance.chevereto_import.username_column.label') }}</label>
        <small>{{ $t('maintenance.chevereto_import.username_column.help') }}</small>
        <InputText v-model="users.username_column" size="small" fluid :disabled />
      </div>
      <div class="field">
        <label>{{ $t('maintenance.chevereto_import.subject_column.label') }}</label>
        <small>{{ $t('maintenance.chevereto_import.subject_column.help') }}</small>
        <InputText v-model="users.subject_column" size="small" fluid :disabled />
      </div>
      <div class="field">
        <label>{{ $t('maintenance.chevereto_import.email_column.label') }}</label>
        <small>{{ $t('maintenance.chevereto_import.email_column.help') }}</small>
        <InputText v-model="emailColumn" size="small" fluid :disabled />
      </div>
    </template>

    <h3>{{ $t('maintenance.chevereto_import.run') }}</h3>
    <div class="field">
      <label>{{ $t('maintenance.chevereto_import.fallback_username.label') }}</label>
      <small>{{ $t('maintenance.chevereto_import.fallback_username.help') }}</small>
      <InputText v-model="form.fallback_username" size="small" fluid :disabled />
    </div>
    <div class="field">
      <label>{{ $t('maintenance.chevereto_import.batch_size.label') }}</label>
      <small>{{ $t('maintenance.chevereto_import.batch_size.help') }}</small>
      <InputNumber v-model="form.batch_size" size="small" :min="1" fluid :disabled />
    </div>
    <div class="field">
      <label>{{ $t('maintenance.chevereto_import.concurrency.label') }}</label>
      <small>{{ $t('maintenance.chevereto_import.concurrency.help') }}</small>
      <InputNumber v-model="form.concurrency" size="small" :min="1" fluid :disabled />
    </div>

    <div class="toolbar">
      <Button
        :label="$t(progress?.last_image_id ? 'maintenance.chevereto_import.resume' : 'maintenance.chevereto_import.start')"
        icon="pi pi-play"
        size="small"
        :loading="progress?.running"
        :disabled="disabled || !form.chevereto_database_url.trim() || !form.chevereto_public_url.trim()"
        @click="start(CheveretoImportStart.Resume)"
      />
      <Button
        :label="$t('maintenance.chevereto_import.retry_failed', { count: progress?.failed ?? 0 })"
        icon="pi pi-replay"
        size="small"
        severity="secondary"
        :disabled="disabled || !progress?.failed || !form.chevereto_database_url.trim()"
        @click="start(CheveretoImportStart.RetryFailed)"
      />
      <Button
        :label="$t('maintenance.chevereto_import.restart')"
        icon="pi pi-refresh"
        size="small"
        severity="secondary"
        outlined
        :disabled="disabled || !progress?.last_image_id || !form.chevereto_database_url.trim()"
        @click="start(CheveretoImportStart.Restart)"
      />
      <Button
        v-if="progress?.running"
        :label="$t('maintenance.chevereto_import.stop')"
        icon="pi pi-stop"
        size="small"
        severity="danger"
        :disabled="stopping"
        @click="stop"
      />
    </div>
    <small>{{ $t('maintenance.chevereto_import.buttons_help') }}</small>

    <template v-if="progress?.started_at">
      <ProgressBar :value="progress.total ? Math.round((progress.done / progress.total) * 100) : 0" :showValue="false" style="height: 6px" />
      <small>{{ $t('maintenance.chevereto_import.progress', { ...progress }) }}</small>
      <small :title="new Date(progress.started_at).toLocaleString()">{{
        $t('maintenance.chevereto_import.started', { when: timeAgo(new Date(progress.started_at)) })
      }}</small>
      <small v-if="progress.running && stopping">{{ $t('maintenance.chevereto_import.stopping') }}</small>
      <Message v-else-if="!progress.running && progress.error" severity="error" size="small">{{ progress.error }}</Message>
      <Message v-else-if="!progress.running && !progress.finished_at" severity="warn" size="small">{{
        $t('maintenance.chevereto_import.interrupted')
      }}</Message>
      <Message v-else-if="!progress.running" severity="success" size="small">{{ $t('maintenance.chevereto_import.finished') }}</Message>
    </template>

    <template v-if="failures && failures.total_items > 0">
      <h3>{{ $t('maintenance.chevereto_import.failures') }}</h3>
      <div class="toolbar">
        <PageSizeSelect
          :modelValue="pageSize"
          @update:modelValue="(size: number) => router.push({ query: { ...route.query, page: undefined, page_size: size } })"
        />
      </div>
      <PaginatedResults
        :page="failures.page"
        :pageSize
        :totalItems="failures.total_items"
        @changePage="(number: number) => router.push({ query: { ...route.query, page: number } })"
      >
        <DataTable :value="failures.results" dataKey="image_id" size="small">
          <Column field="image_id" :header="$t('maintenance.chevereto_import.image_id')" />
          <Column :header="$t('maintenance.chevereto_import.fetch_url')">
            <template #body="{ data }">
              <a v-if="data.fetch_url" :href="data.fetch_url" target="_blank" rel="noopener">{{ data.fetch_url }}</a>
              <template v-else>-</template>
            </template>
          </Column>
          <Column field="error" :header="$t('maintenance.chevereto_import.error')" />
        </DataTable>
      </PaginatedResults>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import InputText from 'primevue/inputtext'
import InputNumber from 'primevue/inputnumber'
import ToggleSwitch from 'primevue/toggleswitch'
import Button from 'primevue/button'
import ProgressBar from 'primevue/progressbar'
import Message from 'primevue/message'
import PageSizeSelect from '@/components/PageSizeSelect.vue'
import PaginatedResults from '@/components/PaginatedResults.vue'
import { api } from '@/services/api/http'
import { timeAgo } from '@/services/format'
import {
  CheveretoImportStart,
  type BackendUsers,
  type CheveretoImportParams,
  type CheveretoImportProgress,
  type PaginatedResultsCheveretoImportFailure,
} from '@/services/api-schema'

const { t } = useI18n()
const form = ref<Omit<CheveretoImportParams, 'start' | 'link_hosts' | 'backend_users'>>({
  chevereto_database_url: '',
  chevereto_public_url: '',
  backend_database_url: '',
  fallback_username: 'chevereto-import',
  batch_size: 100,
  concurrency: 4,
})
const linkHosts = ref('')
const matchUsers = ref(false)
const users = ref<BackendUsers>({ table: 'users', username_column: 'username', subject_column: 'id' })
const emailColumn = ref('email')
const progress = ref<CheveretoImportProgress>()
const failures = ref<PaginatedResultsCheveretoImportFailure>()
const route = useRoute()
const router = useRouter()
const currentPage = computed(() => Number(route.query.page) || 1)
const pageSize = computed(() => Number(route.query.page_size) || 50)
const stopping = ref(false)
const disabled = computed(() => !!progress.value?.running)
let timer: ReturnType<typeof setTimeout> | undefined

const loadFailures = (page: number) =>
  api.listCheveretoImportFailures(page, pageSize.value).then((response) => {
    failures.value = response.data
  })

// Polls once a second while a run goes, then reloads the failures it changed.
const refresh = () => {
  clearTimeout(timer)
  api.getCheveretoImportProgress().then((response) => {
    progress.value = response.data
    if (response.data.running) {
      timer = setTimeout(refresh, 1000)
    } else {
      stopping.value = false
    }
    loadFailures(currentPage.value)
  })
}

const start = (from: CheveretoImportStart) => {
  if (from === CheveretoImportStart.Restart && !confirm(t('maintenance.chevereto_import.restart_confirm'))) return
  const backendDatabaseUrl = form.value.backend_database_url?.trim()
  api
    .startCheveretoImport({
      ...form.value,
      chevereto_database_url: form.value.chevereto_database_url.trim(),
      chevereto_public_url: form.value.chevereto_public_url.trim(),
      backend_database_url: backendDatabaseUrl || null,
      link_hosts: linkHosts.value
        .split(',')
        .map((host) => host.trim())
        .filter(Boolean),
      backend_users: backendDatabaseUrl && matchUsers.value ? { ...users.value, email_column: emailColumn.value.trim() || null } : null,
      start: from,
    })
    .then(refresh)
}

const stop = () =>
  api.stopCheveretoImport().then(() => {
    stopping.value = true
  })

// Post flush: switching tabs unmounts this one before it would reload.
watch(
  () => route.query,
  () => loadFailures(currentPage.value),
  { flush: 'post' },
)
onMounted(refresh)
onUnmounted(() => clearTimeout(timer))
</script>

<style scoped>
.chevereto-import {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 720px;
}
.chevereto-import small {
  margin: 0;
  color: var(--p-text-muted-color);
}
h3 {
  margin: 12px 0 0;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
</style>
