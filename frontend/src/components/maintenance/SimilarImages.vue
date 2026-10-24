<template>
  <div class="similar">
    <small>{{ $t('maintenance.similar_images.help') }}</small>
    <div class="toolbar">
      <label class="distance">
        {{ $t('maintenance.similar_images.max_distance') }}
        <InputNumber v-model="maxDistance" size="small" :min="0" :max="16" showButtons :disabled="maxDistance === undefined" inputClass="distance-input" />
      </label>
      <Button :label="$t('maintenance.similar_images.scan')" icon="pi pi-search" size="small" :loading="progress?.running" @click="scan" />
    </div>
    <template v-if="progress?.running">
      <RunProgress :progress />
      <small>{{ $t('maintenance.similar_images.progress', { done: progress.done, total: progress.total }) }}</small>
    </template>
    <Message v-else-if="progress?.error" severity="error" size="small">{{ progress.error }}</Message>
    <small v-else-if="progress?.scanned_at && page" :title="formatDate(progress.scanned_at)">
      {{ $t('maintenance.similar_images.scanned_at', { when: timeAgo(progress.scanned_at), count: page.total_items }, page.total_items) }}
    </small>
    <template v-if="page && page.total_items > 0">
      <div class="toolbar">
        <Button
          :label="$t('trash.delete', { count: selected.size }, selected.size)"
          icon="pi pi-trash"
          size="small"
          severity="danger"
          :disabled="selected.size === 0"
          @click="deleteDialogVisible = true"
        />
        <PageSizeSelect
          :modelValue="pageSize"
          @update:modelValue="(size: number) => router.push({ query: { ...route.query, page: undefined, page_size: size } })"
        />
      </div>
      <PaginatedResults
        :page="page.page"
        :pageSize
        :totalItems="page.total_items"
        @changePage="(number: number) => router.push({ query: { ...route.query, page: number } })"
      >
        <div class="groups">
          <div v-for="group in page.results" :key="group.images[0].id" class="group">
            <div v-for="image in group.images" :key="image.id" class="image" :class="{ selected: selected.has(image.id) }">
              <router-link :to="{ name: 'Image', params: { id: image.id } }" class="thumb">
                <img :src="image.thumbnail_url" :alt="image.id" loading="lazy" />
              </router-link>
              <label class="details">
                <Checkbox :modelValue="selected.has(image.id)" binary size="small" @update:modelValue="toggleSelected(image.id)" />
                <span>
                  {{ image.width }}x{{ image.height }}, {{ formatBytes(image.size) }}, {{ image.ext }}<br />
                  {{ image.uploader_username }},
                  <span :title="formatDate(image.created_at)">{{ timeAgo(image.created_at) }}</span>
                </span>
              </label>
            </div>
          </div>
        </div>
      </PaginatedResults>
    </template>
    <p v-else-if="page && !progress?.running" class="summary">{{ $t('maintenance.similar_images.empty') }}</p>
    <DeleteImagesDialog v-model:visible="deleteDialogVisible" :ids="[...selected]" @deleted="onDeleted" />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import InputNumber from 'primevue/inputnumber'
import Button from 'primevue/button'
import Checkbox from 'primevue/checkbox'
import Message from 'primevue/message'
import RunProgress from '@/components/RunProgress.vue'
import PageSizeSelect from '@/components/PageSizeSelect.vue'
import PaginatedResults from '@/components/PaginatedResults.vue'
import DeleteImagesDialog from '@/components/DeleteImagesDialog.vue'
import { api } from '@/services/api/http'
import { formatBytes, formatDate, timeAgo } from '@/services/format'
import { pollProgress } from '@/services/poll'
import { type PaginatedResultsSimilarImagesGroup, type SimilarProgress } from '@/services/api-schema'

const route = useRoute()
const router = useRouter()
const currentPage = computed(() => Number(route.query.page) || 1)
// Groups per page.
const pageSize = computed(() => Number(route.query.page_size) || 25)
// The last scan's, from the first progress poll.
const maxDistance = ref<number>()
const progress = ref<SimilarProgress>()
const page = ref<PaginatedResultsSimilarImagesGroup>()
const selected = ref(new Set<string>())

const load = (number: number) =>
  api.listSimilarImages(number, pageSize.value).then((response): unknown => {
    // Trashing or a new scan can empty the page: go to the last one.
    if (response.data.results.length === 0 && number > 1)
      return router.replace({ query: { ...route.query, page: Math.max(1, Math.ceil(response.data.total_items / pageSize.value)) } })
    page.value = response.data
    selected.value.clear()
  })

// Reloads the groups a finished scan replaced.
const refresh = pollProgress(api.getSimilarProgress, progress, () => load(currentPage.value))
watch(progress, (value) => (maxDistance.value ??= value?.max_distance))

const scan = () => {
  if (progress.value?.running || maxDistance.value === undefined) return
  const max_distance = maxDistance.value
  api.scanSimilarImages({ max_distance }).then(() => {
    // Mark the scan as started so the first poll sees it end, even when it finishes before that poll.
    progress.value = { ...progress.value!, running: true, done: 0, total: 0, error: null, max_distance }
    refresh()
  })
}

const toggleSelected = (id: string) => selected.value.delete(id) || selected.value.add(id)

const deleteDialogVisible = ref(false)
const onDeleted = () => {
  load(currentPage.value)
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
.similar {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.similar small,
.summary {
  margin: 0;
  color: var(--p-text-muted-color);
}
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.distance {
  display: flex;
  align-items: center;
  gap: 8px;
}
.distance > :deep(.p-inputnumber) {
  flex-shrink: 0;
}
.distance :deep(.distance-input) {
  width: 6rem;
}
.groups {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.group {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 12px;
  border: 2px solid var(--p-text-muted-color);
  border-radius: 8px;
  background: var(--p-content-hover-background);
}
.image {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 160px;
  padding: 4px;
  border-radius: 6px;
}
.image.selected {
  outline: 2px solid var(--p-red-500);
}
.thumb {
  display: block;
  height: 120px;
  border-radius: 4px;
  overflow: hidden;
  background: var(--p-content-border-color);
}
.thumb img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.details {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  font-size: 0.8rem;
  cursor: pointer;
}
</style>
