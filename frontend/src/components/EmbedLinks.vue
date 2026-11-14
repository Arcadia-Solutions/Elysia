<template>
  <template v-for="group in groups" :key="group.title">
    <h3 class="title">{{ group.title }}</h3>
    <CopyLink v-for="row in group.rows" :key="row.label" :label="row.label" :value="row.value" />
  </template>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import CopyLink from '@/components/CopyLink.vue'
import { absoluteUrl } from '@/services/format'
import type { ChecksumLinks } from '@/services/api-schema'

// Copyable direct links and embed snippets, one line per image. The medium
// embeds link to the full image. Then a single image's content-addressed links, one group per checksum.
const props = defineProps<{ images: { url: string; mediumUrl: string }[]; checksums?: ChecksumLinks[] }>()

const { t } = useI18n()

const groups = computed(() => {
  const images = props.images.map((image) => ({ full: absoluteUrl(image.url), medium: absoluteUrl(image.mediumUrl) }))
  const lines = (render: (full: string, medium: string) => string) => images.map(({ full, medium }) => render(full, medium)).join('\n')
  return [
    {
      title: t('image.links.full'),
      rows: [
        { label: t('image.links.link'), value: lines((full) => full) },
        { label: t('image.links.markdown'), value: lines((full) => `![](${full})`) },
        { label: t('image.links.bbcode'), value: lines((full) => `[img]${full}[/img]`) },
        { label: t('image.links.html'), value: lines((full) => `<img src="${full}">`) },
      ],
    },
    {
      title: t('image.links.medium'),
      rows: [
        { label: t('image.links.link'), value: lines((_, medium) => medium) },
        { label: t('image.links.markdown'), value: lines((full, medium) => `[![](${medium})](${full})`) },
        { label: t('image.links.bbcode'), value: lines((full, medium) => `[url=${full}][img]${medium}[/img][/url]`) },
        { label: t('image.links.html'), value: lines((full, medium) => `<a href="${full}"><img src="${medium}"></a>`) },
      ],
    },
    ...(props.checksums ?? []).map((checksum) => ({
      title: t('image.links.by_checksum', { algorithm: t(`checksums.${checksum.algorithm}`) }),
      rows: [
        { label: t('image.links.full'), value: absoluteUrl(checksum.url) },
        { label: t('image.links.medium'), value: absoluteUrl(checksum.medium_url) },
        { label: t('image.links.thumbnail'), value: absoluteUrl(checksum.thumbnail_url) },
      ],
    })),
  ]
})
</script>

<style scoped>
.title {
  margin: 8px 0 2px;
  font-size: 0.875rem;
  font-weight: 600;
}
</style>
