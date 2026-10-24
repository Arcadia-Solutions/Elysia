<template>
  <div class="uploaded-wrap">
    <div class="previews">
      <router-link v-for="image in images" :key="image.url" :to="{ name: 'Image', params: { id: image.id } }" target="_blank">
        <img :src="image.url" :alt="$t('upload.preview_alt')" class="preview" />
      </router-link>
    </div>
    <EmbedLinks :images="images" />
    <Button :label="$t('upload.another')" size="small" severity="secondary" class="another" @click="router.push({ name: 'Upload' })" />
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeMount } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Button from 'primevue/button'
import EmbedLinks from '@/components/EmbedLinks.vue'

const route = useRoute()
const router = useRouter()

// Reached only via the upload flow, which passes the image and thumbnail URLs
// in the query. A direct visit with no url has nothing to show, so bounce to the form.
onBeforeMount(() => {
  if (!route.query.url) router.replace({ name: 'Upload' })
})

// A single upload yields scalar query params, several yield arrays.
const images = computed(() => {
  const ids = [route.query.id ?? []].flat()
  const thumbnails = [route.query.thumbnailUrl ?? []].flat()
  return [route.query.url ?? []].flat().map((url, i) => ({ id: String(ids[i] ?? ''), url: String(url), thumbnailUrl: String(thumbnails[i] ?? '') }))
})
</script>

<style scoped>
.uploaded-wrap {
  max-width: 600px;
  margin: 0 auto;
  padding: 24px 16px;
  min-height: 80vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 4px;
}
.previews {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 8px;
  align-items: center;
  margin-bottom: 8px;
}
.previews:has(> :only-child) {
  grid-template-columns: 1fr;
}
.preview {
  display: block;
  width: 100%;
  border-radius: 6px;
}
.another {
  margin-top: 12px;
}
</style>
