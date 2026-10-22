<template>
  <div class="unreferenced">
    <small>{{ $t('maintenance.unreferenced_images.help') }}</small>
    <div class="toolbar">
      <InputText v-model="databaseUrl" size="small" class="field" placeholder="postgres://user:password@localhost:5432/app" @keydown.enter="scan" />
      <InputText v-model="hosts" size="small" class="field" :placeholder="$t('maintenance.unreferenced_images.hosts')" @keydown.enter="scan" />
      <InputText v-model="exclude" size="small" class="field" :placeholder="$t('maintenance.unreferenced_images.exclude')" @keydown.enter="scan" />
      <Button
        :label="$t('maintenance.unreferenced_images.scan')"
        icon="pi pi-search"
        size="small"
        :loading="progress?.running"
        :disabled="!databaseUrl.trim() || splitList(hosts).length === 0"
        @click="scan"
      />
    </div>
    <template v-if="progress?.running">
      <ProgressBar :value="progress.total ? Math.round((progress.done / progress.total) * 100) : 0" :showValue="false" style="height: 6px" />
      <small>{{ $t(`maintenance.unreferenced_images.progress.${progress.task}`, { done: progress.done, total: progress.total }) }}</small>
    </template>
    <Message v-else-if="progress?.error" severity="error" size="small">{{ progress.error }}</Message>
    <p v-if="progress && progress.scanned_columns > 0" class="summary">
      {{
        $t('maintenance.unreferenced_images.summary', {
          columns: progress.scanned_columns,
          referenced: progress.referenced_images,
        })
      }}
    </p>
    <template v-if="page && page.total > 0">
      <small v-if="page.scanned_at" :title="new Date(page.scanned_at).toLocaleString()">
        {{ $t('maintenance.unreferenced_images.scanned_at', { when: timeAgo(new Date(page.scanned_at)) }) }}
      </small>
      <div class="toolbar">
        <Button
          :label="$t('trash.delete', { count: selected.length }, selected.length)"
          icon="pi pi-trash"
          size="small"
          severity="danger"
          :disabled="selected.length === 0 || progress?.running"
          @click="trashSelected"
        />
        <Button
          :label="$t('maintenance.unreferenced_images.trash_all', { count: page.total })"
          icon="pi pi-trash"
          size="small"
          severity="danger"
          outlined
          :disabled="progress?.running"
          @click="trashAll"
        />
      </div>
      <DataTable
        v-model:selection="selected"
        :value="page.images"
        dataKey="id"
        lazy
        paginator
        :rows="page.per_page"
        :totalRecords="page.total"
        :first="(pageNumber - 1) * page.per_page"
        :sortField="sortBy"
        :sortOrder="order === SortOrder.Asc ? 1 : -1"
        size="small"
        @page="(event: DataTablePageEvent) => load(event.page + 1)"
        @sort="(event: DataTableSortEvent) => setSort(event)"
      >
        <Column selectionMode="multiple" />
        <Column :header="$t('maintenance.unreferenced_images.image')">
          <template #body="{ data }">
            <router-link :to="{ name: 'Image', params: { id: data.id } }">
              <img :src="data.thumbnail_url" :alt="data.id" class="thumbnail" loading="lazy" />
            </router-link>
          </template>
        </Column>
        <Column field="original_name" :header="$t('maintenance.unreferenced_images.original_name')">
          <template #body="{ data }">{{ data.original_name ?? '-' }}</template>
        </Column>
        <Column field="uploader_username" :header="$t('maintenance.unreferenced_images.uploader')" />
        <Column :field="ImagesSortBy.CreatedAt" :header="$t('maintenance.unreferenced_images.uploaded_at')" sortable>
          <template #body="{ data }">
            <span :title="new Date(data.created_at).toLocaleString()">{{ timeAgo(new Date(data.created_at)) }}</span>
          </template>
        </Column>
        <Column :field="ImagesSortBy.Size" :header="$t('maintenance.unreferenced_images.size')" sortable>
          <template #body="{ data }">{{ formatBytes(data.size) }}</template>
        </Column>
        <Column :field="ImagesSortBy.Width" :header="$t('maintenance.unreferenced_images.width')" sortable />
        <Column :field="ImagesSortBy.Height" :header="$t('maintenance.unreferenced_images.height')" sortable />
        <Column field="ext" :header="$t('maintenance.unreferenced_images.format')" />
        <Column :field="ImagesSortBy.Views" :header="$t('maintenance.unreferenced_images.views')" sortable />
      </DataTable>
    </template>
    <p v-else-if="page && !progress?.running" class="summary">{{ $t('maintenance.unreferenced_images.empty') }}</p>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import DataTable, { type DataTablePageEvent, type DataTableSortEvent } from 'primevue/datatable'
import Column from 'primevue/column'
import InputText from 'primevue/inputtext'
import Button from 'primevue/button'
import ProgressBar from 'primevue/progressbar'
import Message from 'primevue/message'
import { api } from '@/services/api/http'
import { formatBytes, timeAgo } from '@/services/format'
import { showToast } from '@/services/toast'
import {
  ImagesSortBy,
  SortOrder,
  UnreferencedTask,
  type UnreferencedImage,
  type UnreferencedImagesPage,
  type UnreferencedProgress,
} from '@/services/api-schema'

const { t } = useI18n()
const databaseUrl = ref('')
const hosts = ref('')
const exclude = ref('')
const progress = ref<UnreferencedProgress>()
const page = ref<UnreferencedImagesPage>()
const pageNumber = ref(1)
const sortBy = ref<ImagesSortBy>(ImagesSortBy.CreatedAt)
const order = ref<SortOrder>(SortOrder.Desc)
const selected = ref<UnreferencedImage[]>([])
let timer: ReturnType<typeof setTimeout> | undefined

const load = (number: number) =>
  api.listUnreferencedImages(number, sortBy.value, order.value).then((response): unknown => {
    // Trashing the last rows of the last page empties it.
    if (response.data.images.length === 0 && number > 1) return load(number - 1)
    pageNumber.value = number
    page.value = response.data
    // Prefill with the host this instance's links use, relative links use the frontend's.
    if (!hosts.value) hosts.value = response.data.public_host || window.location.host
    selected.value = []
  })

const setSort = (event: DataTableSortEvent) => {
  sortBy.value = event.sortField as ImagesSortBy
  order.value = event.sortOrder === 1 ? SortOrder.Asc : SortOrder.Desc
  load(1)
}

// Polls once a second while a run goes, then reloads the list it changed.
const refresh = () => {
  clearTimeout(timer)
  api.getUnreferencedProgress().then((response) => {
    const wasRunning = progress.value?.running
    progress.value = response.data
    if (response.data.running) {
      timer = setTimeout(refresh, 1000)
    } else if (wasRunning) {
      if (!response.data.error && response.data.task === UnreferencedTask.Trash) {
        showToast(t('trash.deleted', { count: response.data.done }, response.data.done), 'success')
      }
      load(1)
    }
  })
}

// Mark the run as started so the first poll sees it end, even when it finishes before that poll.
// A scan resets the last scan's summary, a trash run keeps it.
const markStarted = (task: UnreferencedTask) => {
  const kept = task === UnreferencedTask.Trash ? progress.value : undefined
  progress.value = { scanned_columns: 0, referenced_images: 0, ...kept, running: true, task, done: 0, total: 0, error: null }
  refresh()
}

const splitList = (text: string) =>
  text
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean)

const scan = () => {
  if (!databaseUrl.value.trim() || splitList(hosts.value).length === 0 || progress.value?.running) return
  api
    .scanUnreferencedImages({ database_url: databaseUrl.value.trim(), hosts: splitList(hosts.value), exclude: splitList(exclude.value) })
    .then(() => markStarted(UnreferencedTask.Scan))
}

const trashSelected = () => {
  const count = selected.value.length
  if (!confirm(t('trash.delete_confirm', { count }, count))) return
  api.trashImages({ ids: selected.value.map((image) => image.id) }).then(() => {
    showToast(t('trash.deleted', { count }, count), 'success')
    load(pageNumber.value)
  })
}

const trashAll = () => {
  const count = page.value?.total ?? 0
  if (!confirm(t('trash.delete_confirm', { count }, count))) return
  api.trashUnreferencedImages().then(() => markStarted(UnreferencedTask.Trash))
}

onMounted(() => {
  refresh()
  load(1)
})
onUnmounted(() => clearTimeout(timer))
</script>

<style scoped>
.unreferenced {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.unreferenced small,
.summary {
  margin: 0;
  color: var(--p-text-muted-color);
}
.toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.field {
  flex: 1;
  min-width: 240px;
}
.thumbnail {
  display: block;
  width: 48px;
  height: 48px;
  object-fit: cover;
  border-radius: 4px;
}
</style>
