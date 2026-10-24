<template>
  <Select
    v-model="albumId"
    size="small"
    fluid
    filter
    showClear
    :disabled
    :placeholder
    :options="albums.filter((album) => !exclude?.includes(album.id))"
    :optionLabel="(album: PaginatedResultsAlbumSummaryResultsInner) => (mine || !isAdmin() ? album.name : `${album.name} (${album.owner_username})`)"
    optionValue="id"
  >
    <template #footer>
      <Button :label="$t('albums.new')" icon="pi pi-plus" size="small" text fluid @click="dialogVisible = true" />
    </template>
  </Select>
  <AlbumDialog v-model:visible="dialogVisible" @saved="created" />
</template>

<script setup lang="ts">
import { ref } from 'vue'
import Select from 'primevue/select'
import Button from 'primevue/button'
import AlbumDialog from '@/components/AlbumDialog.vue'
import { api } from '@/services/api/http'
import { isAdmin } from '@/services/session'
import { AlbumsSortBy, SortOrder, type Album, type PaginatedResultsAlbumSummaryResultsInner } from '@/services/api-schema'

// Picks one of the caller's albums (`mine`), or of everyone's for an admin,
// with a "New album" entry creating one of the caller's and picking it.
const props = defineProps<{ mine: boolean; exclude?: string[]; disabled?: boolean; placeholder?: string }>()
const albumId = defineModel<string | null>({ required: true })
const albums = ref<PaginatedResultsAlbumSummaryResultsInner[]>([])
const dialogVisible = ref(false)

// ponytail: first 500 albums only (the page size cap), page or search server side if someone gets more.
const load = () =>
  api.listAlbums(1, 500, AlbumsSortBy.Name, SortOrder.Asc, undefined, props.mine).then((response) => {
    albums.value = response.data.results
  })

const created = (album: Album) =>
  load().then(() => {
    albumId.value = album.id
  })

load()
</script>
