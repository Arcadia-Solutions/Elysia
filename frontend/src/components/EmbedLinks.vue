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

// Copyable direct links and embed snippets, one line per image. The thumbnail
// embeds link to the full image.
const props = defineProps<{ images: { url: string; thumbnailUrl: string }[] }>()

const { t } = useI18n()

const groups = computed(() => {
  const images = props.images.map((image) => ({ full: absoluteUrl(image.url), thumb: absoluteUrl(image.thumbnailUrl) }))
  const lines = (render: (full: string, thumb: string) => string) => images.map(({ full, thumb }) => render(full, thumb)).join('\n')
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
      title: t('image.links.thumbnail'),
      rows: [
        { label: t('image.links.link'), value: lines((_, thumb) => thumb) },
        { label: t('image.links.markdown'), value: lines((full, thumb) => `[![](${thumb})](${full})`) },
        { label: t('image.links.bbcode'), value: lines((full, thumb) => `[url=${full}][img]${thumb}[/img][/url]`) },
        { label: t('image.links.html'), value: lines((full, thumb) => `<a href="${full}"><img src="${thumb}"></a>`) },
      ],
    },
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
