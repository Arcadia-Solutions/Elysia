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
        <dt>{{ $t('image.dimensions') }}</dt>
        <dd>{{ meta.width }} × {{ meta.height }}</dd>
        <dt>{{ $t('image.size') }}</dt>
        <dd>{{ formatBytes(meta.size) }}</dd>
        <dt>{{ $t('image.format') }}</dt>
        <dd>{{ meta.mime }}</dd>
        <dt>{{ $t('image.views') }}</dt>
        <dd>{{ $t('image.views_value', { views: meta.views, thumbnail_views: meta.thumbnail_views }) }}</dd>
        <dt>{{ $t('image.processing.title') }}</dt>
        <dd>
          <ul v-if="processing.length" class="processing">
            <li v-for="step in processing" :key="step">{{ step }}</li>
          </ul>
          <template v-else>{{ $t('image.processing.none') }}</template>
        </dd>
      </dl>
      <template v-for="group in linkGroups" :key="group.title">
        <h3>{{ group.title }}</h3>
        <CopyLink v-for="row in group.rows" :key="row.label" :label="row.label" :value="row.value" />
      </template>
    </aside>
  </div>
</template>

<script setup lang="ts">
import { absoluteUrl, formatBytes, timeAgo } from '@/services/format'
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'
import CopyLink from '@/components/CopyLink.vue'
import { api } from '@/services/api/http'
import type { MetadataResponse } from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
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
  if (!meta.value) return []
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

api.metadata(String(route.params.id)).then((response) => {
  meta.value = response.data
})
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
.processing {
  margin: 0;
  padding-left: 18px;
}
.side h3 {
  margin: 8px 0 2px;
  font-size: 0.875rem;
  font-weight: 600;
}
.meta dd {
  margin: 0 0 12px;
  word-break: break-word;
}
</style>
