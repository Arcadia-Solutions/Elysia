<template>
  <div class="uploaded-wrap">
    <img :src="link" :alt="$t('upload.preview_alt')" class="preview" />
    <CopyLink :label="$t('upload.image_link')" :link="link" />
    <CopyLink :label="$t('upload.thumbnail_link')" :link="String(route.query.thumbnailUrl ?? '')" />
    <Button :label="$t('upload.another')" size="small" severity="secondary" @click="router.push({ name: 'Upload' })" />
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeMount } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Button from 'primevue/button'
import CopyLink from '@/components/CopyLink.vue'

const route = useRoute()
const router = useRouter()

// Reached only via the upload flow, which passes the image and thumbnail URLs
// in the query. A direct visit with no url has nothing to show, so bounce to the form.
onBeforeMount(() => {
  if (!route.query.url) router.replace({ name: 'Upload' })
})

const link = computed(() => String(route.query.url ?? ''))
</script>

<style scoped>
.uploaded-wrap {
  max-width: 600px;
  margin: 0 auto;
  padding: 0 16px;
  min-height: 80vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 12px;
}
.preview {
  max-width: 100%;
  border-radius: 6px;
}
</style>
