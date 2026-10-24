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
      <RunProgress :progress />
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
    <template v-if="page && page.total_items > 0">
      <small v-if="page.scanned_at" :title="formatDate(page.scanned_at)">
        {{ $t('maintenance.unreferenced_images.scanned_at', { when: timeAgo(page.scanned_at) }) }}
      </small>
      <div class="toolbar">
        <Button
          :label="$t('trash.delete', { count: selected.size }, selected.size)"
          icon="pi pi-trash"
          size="small"
          severity="danger"
          :disabled="selected.size === 0 || progress?.running"
          @click="deleteDialogVisible = true"
        />
        <Button
          :label="$t('maintenance.unreferenced_images.trash_all', { count: page.total_items })"
          icon="pi pi-trash"
          size="small"
          severity="danger"
          outlined
          :disabled="progress?.running"
          @click="trashAll"
        />
      </div>
      <ImageResults
        v-model:selected="selected"
        :images="page.results"
        :page="page.page"
        :pageSize
        :totalItems="page.total_items"
        :sortBy
        :order
        @changePage="(number: number) => router.push({ query: { ...route.query, page: number } })"
        @changePageSize="(size: number) => router.push({ query: { ...route.query, page: undefined, page_size: size } })"
        @sort="(sort_by: ImagesSortBy, order: SortOrder) => router.push({ query: { tab: route.query.tab, page_size: route.query.page_size, sort_by, order } })"
      />
    </template>
    <p v-else-if="page && !progress?.running" class="summary">{{ $t('maintenance.unreferenced_images.empty') }}</p>
    <DeleteImagesDialog v-model:visible="deleteDialogVisible" :ids="[...selected]" @deleted="onDeleted" />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import InputText from 'primevue/inputtext'
import Button from 'primevue/button'
import Message from 'primevue/message'
import RunProgress from '@/components/RunProgress.vue'
import ImageResults from '@/components/ImageResults.vue'
import DeleteImagesDialog from '@/components/DeleteImagesDialog.vue'
import { api } from '@/services/api/http'
import { formatDate, splitList, timeAgo } from '@/services/format'
import { pollProgress } from '@/services/poll'
import { showToast } from '@/services/toast'
import { ImagesSortBy, SortOrder, UnreferencedTask, type UnreferencedImagesPage, type UnreferencedProgress } from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const databaseUrl = ref('')
const hosts = ref('')
const exclude = ref('')
const progress = ref<UnreferencedProgress>()
const page = ref<UnreferencedImagesPage>()
const currentPage = computed(() => Number(route.query.page) || 1)
const pageSize = computed(() => Number(route.query.page_size) || 50)
const sortBy = computed(() => (route.query.sort_by as ImagesSortBy | undefined) ?? ImagesSortBy.CreatedAt)
const order = computed(() => (route.query.order as SortOrder | undefined) ?? SortOrder.Desc)
const selected = ref(new Set<string>())

const load = (number: number) =>
  api.listUnreferencedImages(number, pageSize.value, sortBy.value, order.value).then((response): unknown => {
    // Trashing or a new scan can empty the page: go to the last one.
    if (response.data.results.length === 0 && number > 1)
      return router.replace({ query: { ...route.query, page: Math.max(1, Math.ceil(response.data.total_items / pageSize.value)) } })
    page.value = response.data
    // Prefill with the host this instance's links use, relative links use the frontend's.
    if (!hosts.value) hosts.value = response.data.public_host || window.location.host
    selected.value.clear()
  })

// Reloads the list a finished run changed.
const refresh = pollProgress(api.getUnreferencedProgress, progress, (ended) => {
  if (!ended.error && ended.task === UnreferencedTask.Trash) showToast(t('trash.deleted', { count: ended.done }, ended.done), 'success')
  load(currentPage.value)
})

// Mark the run as started so the first poll sees it end, even when it finishes before that poll.
// A scan resets the last scan's summary, a trash run keeps it.
const markStarted = (task: UnreferencedTask) => {
  const kept = task === UnreferencedTask.Trash ? progress.value : undefined
  progress.value = { scanned_columns: 0, referenced_images: 0, ...kept, running: true, task, done: 0, total: 0, error: null }
  refresh()
}

const scan = () => {
  if (!databaseUrl.value.trim() || splitList(hosts.value).length === 0 || progress.value?.running) return
  api
    .scanUnreferencedImages({ database_url: databaseUrl.value.trim(), hosts: splitList(hosts.value), exclude: splitList(exclude.value) })
    .then(() => markStarted(UnreferencedTask.Scan))
}

const deleteDialogVisible = ref(false)
const onDeleted = () => {
  load(currentPage.value)
}

const trashAll = () => {
  const count = page.value?.total_items ?? 0
  if (!confirm(t('trash.delete_confirm', { count }, count))) return
  api.trashUnreferencedImages().then(() => markStarted(UnreferencedTask.Trash))
}

// Post flush: switching tabs unmounts this one before it would reload.
watch(
  () => route.query,
  () => load(currentPage.value),
  { flush: 'post' },
)
onMounted(() => {
  refresh()
  load(currentPage.value)
})
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
</style>
