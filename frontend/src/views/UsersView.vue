<template>
  <div class="users">
    <h2>{{ $t('users.title') }}</h2>
    <div class="toolbar">
      <InputText v-model="search" size="small" :placeholder="$t('users.search_placeholder')" class="search" />
      <PageSizeSelect v-model="pageSize" @update:modelValue="load(1)" />
    </div>
    <PaginatedResults :page :pageSize :totalItems="total" @changePage="load">
      <!-- Lazy: the server sorts and pages, so a header or page click reloads. -->
      <DataTable
        :value="users"
        lazy
        :loading
        :sortField="sortBy"
        :sortOrder="order === SortOrder.Asc ? 1 : -1"
        size="small"
        selectionMode="single"
        @sort="onSort"
        @rowSelect="(event: DataTableRowSelectEvent<PaginatedResultsUserSummaryResultsInner>) => router.push({ name: 'User', params: { id: event.data.id } })"
      >
        <Column field="username" :header="$t('account.username')" sortable />
        <Column :header="$t('account.email')">
          <template #body="{ data }">{{ data.email ?? '-' }}</template>
        </Column>
        <Column :header="$t('account.provider')">
          <template #body="{ data }">{{ data.provider === 'sso' ? $t('account.provider_sso') : $t('account.provider_local') }}</template>
        </Column>
        <Column :header="$t('account.role')">
          <template #body="{ data }">{{ data.is_admin ? $t('account.role_admin') : $t('account.role_user') }}</template>
        </Column>
        <Column :header="$t('account.status')">
          <template #body="{ data }">{{ data.banned ? $t('account.banned') : $t('account.active') }}</template>
        </Column>
        <Column field="uploads" :header="$t('account.uploads_count')" sortable />
        <Column field="storage_used_bytes" :header="$t('account.storage')" sortable>
          <template #body="{ data }">{{ formatBytes(data.storage_used_bytes) }}</template>
        </Column>
        <Column field="created_at" :header="$t('account.registered_at')" sortable>
          <template #body="{ data }">{{ timeAgo(data.created_at) }}</template>
        </Column>
        <Column field="last_seen" :header="$t('account.last_seen')" sortable>
          <template #body="{ data }">{{ data.last_seen ? timeAgo(data.last_seen) : '-' }}</template>
        </Column>
      </DataTable>
    </PaginatedResults>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import DataTable, { type DataTableRowSelectEvent, type DataTableSortEvent } from 'primevue/datatable'
import Column from 'primevue/column'
import InputText from 'primevue/inputtext'
import PageSizeSelect from '@/components/PageSizeSelect.vue'
import PaginatedResults from '@/components/PaginatedResults.vue'
import { api } from '@/services/api/http'
import { formatBytes, timeAgo } from '@/services/format'
import { SortOrder, UsersSortBy, type PaginatedResultsUserSummaryResultsInner } from '@/services/api-schema'

const router = useRouter()
const users = ref<PaginatedResultsUserSummaryResultsInner[]>([])
const search = ref('')
const sortBy = ref<UsersSortBy>(UsersSortBy.CreatedAt)
const order = ref<SortOrder>(SortOrder.Desc)
const page = ref(1)
const pageSize = ref(50)
const total = ref(0)
const loading = ref(false)

// Requests can come back out of order while typing; only the latest one is shown.
let latest = 0
const load = (target: number) => {
  const request = ++latest
  loading.value = true
  api
    .listUsers(target, pageSize.value, sortBy.value, order.value, search.value.trim() || undefined)
    .then((response) => {
      if (request !== latest) return
      users.value = response.data.results
      page.value = response.data.page
      total.value = response.data.total_items
    })
    .finally(() => {
      if (request === latest) loading.value = false
    })
}

const onSort = (event: DataTableSortEvent) => {
  sortBy.value = event.sortField as UsersSortBy
  order.value = event.sortOrder === 1 ? SortOrder.Asc : SortOrder.Desc
  load(1)
}

let debounce: ReturnType<typeof setTimeout> | undefined
watch(search, () => {
  clearTimeout(debounce)
  loading.value = true
  debounce = setTimeout(() => load(1), 250)
})
load(1)
</script>

<style scoped>
.users {
  max-width: 1100px;
  margin: 0 auto;
  padding: 20px;
}
.users h2 {
  margin-top: 0;
}
.toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 12px;
}
.search {
  flex: 1;
  max-width: 360px;
}
</style>
