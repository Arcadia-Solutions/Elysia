<template>
  <ResultsControls
    :sortBy
    :order
    :pageSize
    :sortOptions="Object.values(ImagesSortBy).map((value) => ({ label: $t(`browse.sort.${value}`), value }))"
    @sort="(value: ImagesSortBy, order: SortOrder) => emit('sort', value, order)"
    @changePageSize="(size: number) => emit('changePageSize', size)"
  />
  <PaginatedResults :page :pageSize :totalItems @changePage="(page: number) => emit('changePage', page)">
    <div v-if="display === 'grid'" class="grid" :class="{ selecting: selected.size > 0 }">
      <div v-for="image in images" :key="image.id" class="cell" :class="{ selected: selected.has(image.id) }">
        <!-- While a selection is in progress, a click on the image selects it too. -->
        <button v-if="selected.size > 0" type="button" class="thumb" tabindex="-1" @click="toggleSelected(image.id)">
          <img :src="image.thumbnail_url" :alt="image.id" loading="lazy" />
        </button>
        <router-link v-else :to="{ name: 'Image', params: { id: image.id } }" class="thumb">
          <img :src="image.thumbnail_url" :alt="image.id" loading="lazy" />
        </router-link>
        <button
          v-if="!readonly"
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
    <DataTable
      v-else
      :selection="images.filter((image) => selected.has(image.id))"
      :value="images"
      dataKey="id"
      lazy
      :sortField="sortBy"
      :sortOrder="order === SortOrder.Asc ? 1 : -1"
      size="small"
      @update:selection="setPageSelection"
      @sort="(event: DataTableSortEvent) => emit('sort', event.sortField as ImagesSortBy, event.sortOrder === 1 ? SortOrder.Asc : SortOrder.Desc)"
    >
      <Column v-if="!readonly" selectionMode="multiple" />
      <Column :header="$t('browse.columns.image')">
        <template #body="{ data }">
          <router-link :to="{ name: 'Image', params: { id: data.id } }">
            <img :src="data.thumbnail_url" :alt="data.id" class="thumbnail" loading="lazy" />
          </router-link>
        </template>
      </Column>
      <Column field="original_name" :header="$t('browse.columns.original_name')">
        <template #body="{ data }">{{ data.original_name ?? '-' }}</template>
      </Column>
      <Column field="uploader_username" :header="$t('browse.columns.uploader')" />
      <Column :field="ImagesSortBy.CreatedAt" :header="$t('browse.columns.uploaded_at')" sortable>
        <template #body="{ data }">
          <span :title="formatDate(data.created_at)">{{ timeAgo(data.created_at) }}</span>
        </template>
      </Column>
      <Column :field="ImagesSortBy.Size" :header="$t('browse.columns.size')" sortable>
        <template #body="{ data }">{{ formatBytes(data.size) }}</template>
      </Column>
      <Column :field="ImagesSortBy.Width" :header="$t('browse.columns.width')" sortable />
      <Column :field="ImagesSortBy.Height" :header="$t('browse.columns.height')" sortable />
      <Column field="ext" :header="$t('browse.columns.format')" />
      <Column :field="ImagesSortBy.Views" :header="$t('browse.columns.views')" sortable />
    </DataTable>
  </PaginatedResults>
</template>

<script setup lang="ts">
import DataTable, { type DataTableSortEvent } from 'primevue/datatable'
import Column from 'primevue/column'
import ResultsControls, { display } from '@/components/ResultsControls.vue'
import PaginatedResults from '@/components/PaginatedResults.vue'
import { formatBytes, formatDate, timeAgo } from '@/services/format'
import { ImagesSortBy, SortOrder, type PaginatedResultsImageSummaryResultsInner } from '@/services/api-schema'

const props = defineProps<{
  images: PaginatedResultsImageSummaryResultsInner[]
  page: number
  pageSize: number
  totalItems: number
  sortBy: ImagesSortBy
  order: SortOrder
  // Hides the selection controls, for lists the caller can't act on.
  readonly?: boolean
}>()

const emit = defineEmits<{
  changePage: [number]
  changePageSize: [number]
  sort: [ImagesSortBy, SortOrder]
}>()

// Ids of the selected images, which may span several pages.
const selected = defineModel<Set<string>>('selected', { required: true })

const toggleSelected = (id: string) => selected.value.delete(id) || selected.value.add(id)

// The table only knows the current page: keep the other pages' selection.
const setPageSelection = (rows: PaginatedResultsImageSummaryResultsInner[]) => {
  for (const image of props.images) selected.value.delete(image.id)
  for (const row of rows) selected.value.add(row.id)
}
</script>

<style scoped>
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
.thumbnail {
  display: block;
  width: 48px;
  height: 48px;
  object-fit: cover;
  border-radius: 4px;
}
</style>
