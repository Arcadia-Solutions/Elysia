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
      </dl>
      <template v-for="group in linkGroups" :key="group.title">
        <h3>{{ group.title }}</h3>
        <CopyLink v-for="row in group.rows" :key="row.label" :label="row.label" :value="row.value" />
      </template>
      <Button v-if="meta.can_delete" :label="$t('trash.delete', 1)" icon="pi pi-trash" severity="danger" size="small" class="delete" @click="trash" />
      <Button v-if="meta.trashed" :label="$t('trash.restore', 1)" icon="pi pi-replay" size="small" class="delete" @click="restore" />
    </aside>
  </div>
</template>

<script setup lang="ts">
import { absoluteUrl, formatBytes, timeAgo } from '@/services/format'
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import Button from 'primevue/button'
import CopyLink from '@/components/CopyLink.vue'
import { api } from '@/services/api/http'
import { isAdmin } from '@/services/session'
import { showToast } from '@/services/toast'
import type { MetadataResponse } from '@/services/api-schema'

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

// Copyable direct links and embed snippets. The thumbnail embeds link to the full image.
const linkGroups = computed(() => {
  // Trashed images are not served from their public links.
  if (!meta.value || meta.value.trashed) return []
  const full = absoluteUrl(meta.value.url)
  const thumb = absoluteUrl(meta.value.thumbnail_url)
  return [
    {
      title: t('image.links.full'),
      rows: [
        { label: t('image.links.link'), value: full },
        { label: t('image.links.markdown'), value: `![](${full})` },
        { label: t('image.links.bbcode'), value: `[img]${full}[/img]` },
        { label: t('image.links.html'), value: `<img src="${full}">` },
      ],
    },
    {
      title: t('image.links.thumbnail'),
      rows: [
        { label: t('image.links.link'), value: thumb },
        { label: t('image.links.markdown'), value: `[![](${thumb})](${full})` },
        { label: t('image.links.bbcode'), value: `[url=${full}][img]${thumb}[/img][/url]` },
        { label: t('image.links.html'), value: `<a href="${full}"><img src="${thumb}"></a>` },
      ],
    },
  ]
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
.side h3 {
  margin: 8px 0 2px;
  font-size: 0.875rem;
  font-weight: 600;
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
