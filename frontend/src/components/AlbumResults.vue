<template>
  <ResultsControls
    :sortBy
    :order
    :pageSize
    :sortOptions="
      Object.values(AlbumsSortBy)
        .filter((value) => isAdmin() || value !== AlbumsSortBy.OwnerUsername)
        .map((value) => ({ label: $t(`albums.sort.${value}`), value }))
    "
    @sort="(value: AlbumsSortBy, order: SortOrder) => emit('sort', value, order)"
    @changePageSize="(size: number) => emit('changePageSize', size)"
  />
  <PaginatedResults :page :pageSize :totalItems @changePage="(page: number) => emit('changePage', page)">
    <div v-if="display === 'grid'" class="grid">
      <router-link v-for="album in albums" :key="album.id" :to="{ name: 'Album', params: { id: album.id } }" class="card">
        <span class="name">{{ album.name }}</span>
        <small class="details">
          {{ $t('albums.image_count', { count: album.image_count }, album.image_count) }}
          <template v-if="isAdmin()"> · {{ album.owner_username }}</template>
        </small>
      </router-link>
    </div>
    <DataTable
      v-else
      :value="albums"
      dataKey="id"
      lazy
      :sortField="sortBy"
      :sortOrder="order === SortOrder.Asc ? 1 : -1"
      size="small"
      @sort="(event: DataTableSortEvent) => emit('sort', event.sortField as AlbumsSortBy, event.sortOrder === 1 ? SortOrder.Asc : SortOrder.Desc)"
    >
      <Column :field="AlbumsSortBy.Name" :header="$t('albums.columns.name')" sortable>
        <template #body="{ data }">
          <router-link :to="{ name: 'Album', params: { id: data.id } }" class="name-link">{{ data.name }}</router-link>
        </template>
      </Column>
      <Column v-if="isAdmin()" :field="AlbumsSortBy.OwnerUsername" :header="$t('albums.columns.owner')" sortable />
      <Column :field="AlbumsSortBy.CreatedAt" :header="$t('albums.columns.created_at')" sortable>
        <template #body="{ data }">
          <span :title="formatDate(data.created_at)">{{ timeAgo(data.created_at) }}</span>
        </template>
      </Column>
      <Column :field="AlbumsSortBy.ImageCount" :header="$t('albums.columns.image_count')" sortable />
    </DataTable>
  </PaginatedResults>
</template>

<script setup lang="ts">
import DataTable, { type DataTableSortEvent } from 'primevue/datatable'
import Column from 'primevue/column'
import PaginatedResults from '@/components/PaginatedResults.vue'
import ResultsControls, { display } from '@/components/ResultsControls.vue'
import { formatDate, timeAgo } from '@/services/format'
import { isAdmin } from '@/services/session'
import { AlbumsSortBy, SortOrder, type PaginatedResultsAlbumSummaryResultsInner } from '@/services/api-schema'

defineProps<{
  albums: PaginatedResultsAlbumSummaryResultsInner[]
  page: number
  pageSize: number
  totalItems: number
  sortBy: AlbumsSortBy
  order: SortOrder
}>()

const emit = defineEmits<{
  changePage: [number]
  changePageSize: [number]
  sort: [AlbumsSortBy, SortOrder]
}>()
</script>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 16px;
}
.card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 14px 16px;
  color: inherit;
  text-decoration: none;
  transition: background-color 0.15s;
}
.card:hover {
  background: var(--p-content-hover-background);
}
.name {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.name-link {
  font-weight: 600;
  color: inherit;
  text-decoration: none;
}
.details {
  color: var(--p-text-muted-color);
}
</style>
