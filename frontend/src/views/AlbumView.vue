<template>
  <div v-if="album" class="album">
    <header class="head">
      <h1>{{ album.name }}</h1>
      <small class="details">
        {{ $t('albums.image_count', { count: total }, total) }} ·
        {{ $t('albums.created_by', { owner: album.owner_username }) }}
        <span :title="formatDate(album.created_at)">{{ timeAgo(album.created_at) }}</span>
      </small>
    </header>
    <!-- Anyone may view an album, only its owner and admins change it. -->
    <div v-if="canEdit" class="toolbar">
      <template v-if="selected.size > 0">
        <Button
          :label="$t('albums.remove_selected', { count: selected.size }, selected.size)"
          icon="pi pi-minus-circle"
          size="small"
          severity="danger"
          @click="removeSelected"
        />
        <Button :label="$t('browse.clear_selection')" icon="pi pi-times" size="small" severity="secondary" @click="selected.clear()" />
      </template>
      <Button :label="$t('albums.rename')" icon="pi pi-pencil" size="small" severity="secondary" @click="renameVisible = true" />
      <Button :label="$t('albums.delete')" icon="pi pi-trash" size="small" severity="danger" outlined @click="deleteAlbum" />
    </div>
    <AlbumDialog v-model:visible="renameVisible" :album @saved="(renamed) => (album = renamed)" />
    <p v-if="total === 0" class="empty">{{ $t('albums.empty_album') }}</p>
    <ImageResults
      v-else
      v-model:selected="selected"
      :readonly="!canEdit || !albumsEnabled"
      :images
      :page
      :pageSize
      :totalItems="total"
      :sortBy
      :order
      @changePage="(page: number) => router.push({ query: { ...route.query, page } })"
      @changePageSize="(size: number) => router.push({ query: { ...route.query, page: undefined, page_size: size } })"
      @sort="(sort_by: ImagesSortBy, order: SortOrder) => router.push({ query: { page_size: route.query.page_size, sort_by, order } })"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import Button from 'primevue/button'
import AlbumDialog from '@/components/AlbumDialog.vue'
import ImageResults from '@/components/ImageResults.vue'
import { api } from '@/services/api/http'
import { formatDate, timeAgo } from '@/services/format'
import { isAdmin } from '@/services/session'
import { showToast } from '@/services/toast'
import {
  ImagesSortBy,
  ImagesStatus,
  SortOrder,
  type Album,
  type PaginatedResultsImageSummaryResultsInner,
  type PublicElysiaSettings,
} from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

const album = ref<Album>()
const images = ref<PaginatedResultsImageSummaryResultsInner[]>([])
const page = ref(1)
const total = ref(0)
const renameVisible = ref(false)
// Selection survives page changes, so images from several pages can go at once.
const selected = ref(new Set<string>())

const albumId = computed(() => String(route.params.id))
const canEdit = computed(() => album.value?.mine || isAdmin())
// Images may only be removed while albums are enabled.
const settings: PublicElysiaSettings | null = JSON.parse(localStorage.getItem('public_settings') ?? 'null')
const albumsEnabled = settings?.albums_enabled !== false
const sortBy = computed(() => (route.query.sort_by as ImagesSortBy | undefined) ?? ImagesSortBy.CreatedAt)
const order = computed(() => (route.query.order as SortOrder | undefined) ?? SortOrder.Desc)
const pageSize = computed(() => Number(route.query.page_size) || 100)

const load = (target: number) =>
  api.images(target, pageSize.value, sortBy.value, order.value, ImagesStatus.Available, undefined, albumId.value).then((response) => {
    images.value = response.data.results
    page.value = response.data.page
    total.value = response.data.total_items
  })

const removeSelected = () => {
  const count = selected.value.size
  api.removeAlbumImages(albumId.value, { ids: [...selected.value] }).then(() => {
    showToast(t('albums.removed', { count }, count), 'success')
    selected.value.clear()
    load(page.value)
  })
}

const deleteAlbum = () => {
  if (!album.value || !confirm(t('albums.delete_confirm', { name: album.value.name }))) return
  api.deleteAlbum(albumId.value).then(() => {
    showToast(t('albums.deleted'), 'success')
    router.push({ name: 'Browse', query: { tab: 'albums' } })
  })
}

watch(
  albumId,
  (id) =>
    api.getAlbum(id).then((response) => {
      album.value = response.data
    }),
  { immediate: true },
)
watch(
  () => route.query,
  (query) => load(Number(query.page) || 1),
  { immediate: true },
)
</script>

<style scoped>
.album {
  padding: 24px 16px;
}
.head {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  margin-bottom: 16px;
  text-align: center;
}
.head h1 {
  margin: 0;
  font-size: 1.5rem;
  word-break: break-word;
}
.details,
.empty {
  color: var(--p-text-muted-color);
}
.empty {
  text-align: center;
}
.toolbar {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
  margin-bottom: 16px;
}
</style>
