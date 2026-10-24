<template>
  <div v-if="meta" class="image-wrap">
    <img :src="meta.url" :alt="meta.original_name ?? meta.id" class="image" />
    <aside class="side">
      <dl class="meta">
        <template v-if="meta.original_name">
          <dt>{{ $t('image.name') }}</dt>
          <dd>{{ meta.original_name }}</dd>
        </template>
        <dt>{{ $t('image.uploaded_by') }}</dt>
        <dd>{{ meta.uploader_username }}</dd>
        <dt>{{ $t('image.uploaded_at') }}</dt>
        <dd>
          {{ new Date(meta.created_at).toLocaleString('sv-SE') }}
          <br />
          ({{ timeAgo(new Date(meta.created_at)) }})
        </dd>
        <template v-if="meta.trashed_at">
          <dt>{{ $t('image.trashed_at') }}</dt>
          <dd>
            {{ new Date(meta.trashed_at).toLocaleString('sv-SE') }}
            <br />
            ({{ timeAgo(new Date(meta.trashed_at)) }})
            <br />
            <span class="trashed">{{ $t('image.days_until_deletion', { days: meta.days_until_deletion }, meta.days_until_deletion ?? 0) }}</span>
          </dd>
          <dt>{{ $t('image.trashed_by') }}</dt>
          <dd>{{ meta.trashed_by_username }}</dd>
        </template>
        <dt>{{ $t('image.dimensions') }}</dt>
        <dd>{{ meta.width }} × {{ meta.height }}</dd>
        <dt>{{ $t('image.size') }}</dt>
        <dd>{{ formatBytes(meta.size) }}</dd>
        <dt>{{ $t('image.format') }}</dt>
        <dd>{{ meta.mime }}</dd>
        <dt>{{ $t('image.views') }}</dt>
        <dd>
          {{ $t('image.views_value', { views: meta.views, thumbnail_views: meta.thumbnail_views }) }}
          <ul v-if="meta.referrers.length" class="list">
            <li v-for="row in meta.referrers" :key="row.referrer">
              {{ row.referrer || $t('image.no_referrer') }}: {{ $t('image.views_value', { views: row.views, thumbnail_views: row.thumbnail_views }) }}
            </li>
          </ul>
        </dd>
        <dt>{{ $t('image.processing.title') }}</dt>
        <dd>
          <ul v-if="processing.length" class="list">
            <li v-for="step in processing" :key="step">{{ step }}</li>
          </ul>
          <template v-else>{{ $t('image.processing.none') }}</template>
        </dd>
        <dt>{{ $t('albums.title') }}</dt>
        <dd class="albums">
          <Chip
            v-for="album in showAllAlbums ? meta.albums : meta.albums.slice(0, 10)"
            :key="album.id"
            :removable="album.mine"
            @remove="removeFromAlbum(album)"
          >
            <router-link :to="{ name: 'Album', params: { id: album.id } }">{{ albumLabel(album) }}</router-link>
          </Chip>
          <Button v-if="!showAllAlbums && meta.albums.length > 10" label="..." size="small" severity="secondary" text rounded @click="showAllAlbums = true" />
          <Button
            v-if="!meta.trashed"
            icon="pi pi-plus"
            size="small"
            severity="secondary"
            text
            rounded
            :title="$t('albums.add')"
            :aria-label="$t('albums.add')"
            @click="albumDialogVisible = true"
          />
        </dd>
      </dl>
      <!-- Trashed images are not served from their public links. -->
      <EmbedLinks v-if="!meta.trashed" :images="[{ url: meta.url, thumbnailUrl: meta.thumbnail_url }]" />
      <Button v-if="meta.can_delete" :label="$t('trash.delete', 1)" icon="pi pi-trash" severity="danger" size="small" class="delete" @click="trash" />
      <Button v-if="meta.trashed" :label="$t('trash.restore', 1)" icon="pi pi-replay" size="small" class="delete" @click="restore" />
    </aside>
    <Dialog v-model:visible="albumDialogVisible" modal :header="$t('albums.add')" :style="{ width: '25rem' }">
      <AlbumSelect
        :modelValue="null"
        :mine="false"
        :exclude="meta.albums.map((album) => album.id)"
        :placeholder="$t('albums.album')"
        @update:modelValue="addToAlbum"
      />
    </Dialog>
  </div>
</template>

<script setup lang="ts">
import { formatBytes, timeAgo } from '@/services/format'
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import Button from 'primevue/button'
import Chip from 'primevue/chip'
import Dialog from 'primevue/dialog'
import AlbumSelect from '@/components/AlbumSelect.vue'
import EmbedLinks from '@/components/EmbedLinks.vue'
import { api } from '@/services/api/http'
import { isAdmin } from '@/services/session'
import { showToast } from '@/services/toast'
import type { ImageAlbum, MetadataResponse } from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const meta = ref<MetadataResponse | null>(null)

// What the upload pipeline changed compared to the source file.
const processing = computed(() => {
  const image = meta.value
  if (!image) return []
  const steps: string[] = []
  if (image.ext !== image.original_ext) {
    steps.push(t('image.processing.converted', { from: image.original_ext.toUpperCase(), to: image.ext.toUpperCase() }))
  }
  if (image.width !== image.original_width || image.height !== image.original_height) {
    steps.push(t('image.processing.resized', { width: image.original_width, height: image.original_height }))
  }
  if (image.applied_quality != null) {
    steps.push(t('image.processing.compressed', { quality: image.applied_quality }))
  }
  return steps
})

const trash = () => {
  if (!meta.value || !confirm(t('trash.delete_confirm', 1))) return
  api.trashImages({ ids: [meta.value.id] }).then(() => {
    showToast(t('trash.deleted', 1), 'success')
    router.push({ name: isAdmin() ? 'Browse' : 'Upload' })
  })
}

const restore = () => {
  if (!meta.value) return
  api.restoreImages({ ids: [meta.value.id] }).then(() => {
    showToast(t('trash.restored', 1), 'success')
    load()
  })
}

const albumDialogVisible = ref(false)
const showAllAlbums = ref(false)

const albumLabel = (album: ImageAlbum) => (album.mine ? album.name : `${album.name} (${album.owner_username})`)

const addToAlbum = (albumId: string | null) => {
  if (!meta.value || !albumId) return
  api.addAlbumImages(albumId, { ids: [meta.value.id] }).then(() => {
    albumDialogVisible.value = false
    showToast(t('albums.added'), 'success')
    load()
  })
}

const removeFromAlbum = (album: ImageAlbum) => {
  if (!meta.value) return
  api.removeAlbumImages(album.id, { ids: [meta.value.id] }).then(() => {
    showToast(t('albums.removed', 1), 'success')
    load()
  })
}

const load = () =>
  api.metadata(String(route.params.id)).then((response) => {
    meta.value = response.data
  })

load()
</script>

<style scoped>
.image-wrap {
  min-height: 85vh;
  padding: 24px 16px;
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  align-items: center;
  gap: 24px;
}
.image {
  flex: 1 1 400px;
  min-width: 0;
  max-height: 85vh;
  object-fit: contain;
  border-radius: 6px;
}
.side {
  flex: 0 1 420px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.meta {
  margin: 0;
}
.meta dt {
  color: var(--p-text-muted-color);
  font-size: 0.875rem;
}
.list {
  margin: 0;
  padding-left: 18px;
}
.albums {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}
.albums a {
  font-weight: 600;
  color: inherit;
  text-decoration: none;
}
.delete {
  align-self: flex-start;
  margin-top: 16px;
}
.trashed {
  color: var(--p-red-500);
}
.meta dd {
  margin: 0 0 12px;
  word-break: break-word;
}
</style>
