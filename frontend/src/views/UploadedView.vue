<template>
  <div class="uploaded-wrap">
    <img :src="link" :alt="$t('upload.preview_alt')" class="preview" />
    <div class="link-field">
      <label>{{ $t('upload.image_link') }}</label>
      <div class="link-row">
        <InputText :value="fullLink" size="small" readonly fluid />
        <Button icon="pi pi-copy" size="small" :title="$t('upload.copy')" :aria-label="$t('upload.copy')" @click="copyLink(fullLink)" />
      </div>
    </div>
    <div class="link-field">
      <label>{{ $t('upload.thumbnail_link') }}</label>
      <div class="link-row">
        <InputText :value="fullThumbLink" size="small" readonly fluid />
        <Button icon="pi pi-copy" size="small" :title="$t('upload.copy')" :aria-label="$t('upload.copy')" @click="copyLink(fullThumbLink)" />
      </div>
    </div>
    <Button :label="$t('upload.another')" size="small" severity="secondary" @click="router.push({ name: 'Upload' })" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import InputText from 'primevue/inputtext'
import Button from 'primevue/button'
import { useI18n } from 'vue-i18n'
import { showToast } from '@/services/toast'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

// Reached only via the upload flow, which passes the image and thumbnail URLs
// in the query. A direct visit with no url has nothing to show, so bounce to the form.
if (!route.query.url) router.replace({ name: 'Upload' })

const link = computed(() => String(route.query.url))
const fullLink = computed(() => new URL(link.value, window.location.origin).href)
const fullThumbLink = computed(() => new URL(String(route.query.thumbnailUrl), window.location.origin).href)

const copyLink = (value: string) => {
  navigator.clipboard.writeText(value)
  showToast('', t('upload.copied'), 'success')
}
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
.link-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.link-field label {
  color: var(--p-text-muted-color);
  font-size: 0.875rem;
}
.link-row {
  display: flex;
  gap: 8px;
}
.preview {
  max-width: 100%;
  border-radius: 6px;
}
</style>
