<template>
  <div class="browse">
    <div class="toolbar">
      <SelectButton
        :modelValue="status"
        size="small"
        :allowEmpty="false"
        :options="Object.values(ImagesStatus).map((value) => ({ label: $t(`browse.status.${value}`), value }))"
        option-label="label"
        option-value="value"
        @update:modelValue="(value: ImagesStatus) => router.push({ query: { ...route.query, status: value, page: undefined } })"
      />
      <Select
        :modelValue="sortBy"
        size="small"
        :options="Object.values(ImagesSortBy).map((value) => ({ label: $t(`browse.sort.${value}`), value }))"
        option-label="label"
        option-value="value"
        @update:modelValue="(value: ImagesSortBy) => setSort(value, order)"
      />
      <Button
        :icon="order === SortOrder.Asc ? 'pi pi-sort-amount-up-alt' : 'pi pi-sort-amount-down'"
        size="small"
        severity="secondary"
        :title="$t(`browse.order.${order}`)"
        :aria-label="$t(`browse.order.${order}`)"
        @click="setSort(sortBy, order === SortOrder.Asc ? SortOrder.Desc : SortOrder.Asc)"
      />
      <template v-if="selected.size > 0">
        <Button :label="$t('trash.delete', { count: selected.size }, selected.size)" icon="pi pi-trash" size="small" severity="danger" @click="trashSelected" />
        <Button :label="$t('browse.clear_selection')" icon="pi pi-times" size="small" severity="secondary" @click="selected.clear()" />
      </template>
      <Button
        v-if="status === ImagesStatus.Trashed && total > 0"
        :label="$t('browse.empty_trash')"
        icon="pi pi-trash"
        size="small"
        severity="danger"
        @click="emptyTrash"
      />
    </div>
    <p v-if="total === 0" class="empty">{{ $t(`browse.empty.${status}`) }}</p>
    <template v-else>
      <div class="grid" :class="{ selecting: selected.size > 0 }">
        <div v-for="image in images" :key="image.id" class="cell" :class="{ selected: selected.has(image.id) }">
          <img v-if="status === ImagesStatus.Trashed" :src="trashedThumbnails[image.id]" :alt="image.id" />
          <!-- While a selection is in progress, a click on the image selects it too. -->
          <button v-else-if="selected.size > 0" type="button" class="thumb" tabindex="-1" @click="toggleSelected(image.id)">
            <img :src="image.thumbnail_url" :alt="image.id" loading="lazy" />
          </button>
          <router-link v-else :to="{ name: 'Image', params: { id: image.id } }" class="thumb">
            <img :src="image.thumbnail_url" :alt="image.id" loading="lazy" />
          </router-link>
          <button
            v-if="status === ImagesStatus.Available"
            type="button"
            class="select-indicator"
            :aria-pressed="selected.has(image.id)"
            :aria-label="$t('browse.select')"
            @click="toggleSelected(image.id)"
          >
            <i class="pi pi-check" />
          </button>
        </div>
      </div>
      <Paginator :rows="perPage" :totalRecords="total" :first="(page - 1) * perPage" @page="onPage" />
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import Paginator, { type PageState } from 'primevue/paginator'
import Select from 'primevue/select'
import SelectButton from 'primevue/selectbutton'
import Button from 'primevue/button'
import http, { api } from '@/services/api/http'
import { showToast } from '@/services/toast'
import { ImagesSortBy, ImagesStatus, SortOrder, type ImageSummary } from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

const images = ref<ImageSummary[]>([])
const page = ref(1)
const perPage = ref(100)
const total = ref(0)
// Selection survives page changes, so images from several pages can go at once.
const selected = ref(new Set<string>())
// Object URLs of the trashed thumbnails, by image id. The trash is admin-only,
// so they are fetched with the session token instead of loaded by an <img>.
const trashedThumbnails = ref<Record<string, string>>({})

const sortBy = computed(() => (route.query.sort_by as ImagesSortBy | undefined) ?? ImagesSortBy.CreatedAt)
const order = computed(() => (route.query.order as SortOrder | undefined) ?? SortOrder.Desc)
const status = computed(() => (route.query.status as ImagesStatus | undefined) ?? ImagesStatus.Available)

const loadTrashedThumbnails = () => {
  Object.values(trashedThumbnails.value).forEach(URL.revokeObjectURL)
  trashedThumbnails.value = {}
  if (status.value !== ImagesStatus.Trashed) return
  for (const image of images.value) {
    http.get(image.thumbnail_url, { responseType: 'blob' }).then((response) => {
      trashedThumbnails.value[image.id] = URL.createObjectURL(response.data)
    })
  }
}

const load = (target: number) => {
  api
    .images(target, sortBy.value, order.value, status.value)
    .then((response) => {
      images.value = response.data.images
      page.value = target
      perPage.value = response.data.per_page
      total.value = response.data.total
      loadTrashedThumbnails()
    })
    .catch(() => {
      showToast(t('browse.load_error'), 'error')
    })
}

// A new order starts back at the first page.
const setSort = (sort_by: ImagesSortBy, order: SortOrder) => router.push({ query: { status: route.query.status, sort_by, order } })

const toggleSelected = (id: string) => {
  if (!selected.value.delete(id)) selected.value.add(id)
}

const trashSelected = () => {
  const count = selected.value.size
  if (!confirm(t('trash.delete_confirm', { count }, count))) return
  api.trashImages({ ids: [...selected.value] }).then(() => {
    showToast(t('trash.deleted', { count }, count), 'success')
    selected.value.clear()
    load(page.value)
  })
}

const emptyTrash = () => {
  if (!confirm(t('browse.empty_trash_confirm', { count: total.value }, total.value))) return
  api.emptyTrash().then(() => {
    showToast(t('browse.trash_emptied'), 'success')
    load(1)
  })
}

const onPage = (event: PageState) => router.push({ query: { ...route.query, page: event.page + 1 } })

watch(
  () => route.query,
  (query) => load(Number(query.page) || 1),
  { immediate: true },
)
watch(status, () => selected.value.clear())
</script>

<style scoped>
.browse {
  padding: 24px 16px;
}
.toolbar {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
  margin-bottom: 16px;
}
.empty {
  text-align: center;
  color: var(--p-text-muted-color);
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 8px;
}
.cell {
  position: relative;
  aspect-ratio: 1;
  overflow: hidden;
  border-radius: 6px;
  background: var(--p-content-border-color, #eee);
}
.thumb {
  display: block;
  width: 100%;
  height: 100%;
  padding: 0;
  border: none;
  cursor: pointer;
}
.cell img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.cell.selected {
  outline: 3px solid var(--p-primary-color);
  outline-offset: -3px;
}
.cell.selected img {
  opacity: 0.7;
}
.select-indicator {
  position: absolute;
  top: 6px;
  right: 6px;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border: 2px solid #fff;
  border-radius: 50%;
  background: rgb(0 0 0 / 0.35);
  color: transparent;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.15s;
}
.select-indicator .pi {
  font-size: 0.75rem;
}
/* Shown on hover and keyboard focus, always once a selection is in progress,
   and always on touch screens, which have no hover. */
.cell:hover .select-indicator,
.select-indicator:focus-visible,
.selecting .select-indicator {
  opacity: 1;
}
@media (hover: none) {
  .select-indicator {
    opacity: 1;
  }
}
.cell.selected .select-indicator {
  background: var(--p-primary-color);
  border-color: var(--p-primary-color);
  color: var(--p-primary-contrast-color, #fff);
}
</style>
