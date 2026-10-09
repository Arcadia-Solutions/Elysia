<template>
  <div class="uploaded-wrap">
    <img :src="link" :alt="$t('upload.preview_alt')" class="preview" />
    <div class="link-row">
      <InputText :value="fullLink" size="small" readonly fluid />
      <Button icon="pi pi-copy" size="small" :title="$t('upload.copy')" :aria-label="$t('upload.copy')" @click="copyLink" />
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

// Reached only via the upload flow, which passes id+ext in the query.
// A direct visit with no id has nothing to show, so bounce to the form.
if (!route.query.id) router.replace({ name: 'Upload' })

const link = computed(() => `/i/${route.query.id}.${route.query.ext}`)
const fullLink = computed(() => new URL(link.value, window.location.origin).href)

const copyLink = () => {
  navigator.clipboard.writeText(fullLink.value)
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
.link-row {
  display: flex;
  gap: 8px;
}
.preview {
  max-width: 100%;
  border-radius: 6px;
}
</style>
