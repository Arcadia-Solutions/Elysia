<template>
  <div class="upload-wrap">
    <div
      class="dropzone"
      :class="{ dragging }"
      @dragover.prevent
      @dragenter.prevent="dragging = true"
      @dragleave.prevent="dragging = false"
      @drop.prevent="onDrop"
    >
      <FileUpload mode="basic" accept="image/*" :auto="true" customUpload :chooseLabel="$t('upload.choose')" @uploader="onUpload" :disabled="uploading" />
      <span class="drop-hint">{{ $t('upload.drop_hint') }}</span>
    </div>

    <div class="url-row">
      <InputText v-model="urlInput" size="small" :placeholder="$t('upload.url_placeholder')" fluid @keyup.enter="rehost" :disabled="uploading" />
      <Button :label="$t('upload.rehost')" size="small" @click="rehost" :disabled="uploading || !urlInput" />
    </div>

    <div v-if="result" class="result">
      <img :src="link" :alt="$t('upload.preview_alt')" class="preview" />
      <div class="link-row">
        <InputText :value="fullLink" size="small" readonly fluid />
        <Button icon="pi pi-copy" size="small" :title="$t('upload.copy')" :aria-label="$t('upload.copy')" @click="copyLink" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import FileUpload, { type FileUploadUploaderEvent } from 'primevue/fileupload'
import InputText from 'primevue/inputtext'
import Button from 'primevue/button'
import { useI18n } from 'vue-i18n'
import { api } from '@/services/api/http'
import type { UploadResponse } from '@/services/api-schema'
import { showToast } from '@/services/toast'

const { t } = useI18n()
const result = ref<UploadResponse | null>(null)
const uploading = ref(false)
const dragging = ref(false)
const urlInput = ref('')

const link = computed(() => (result.value ? `/i/${result.value.id}.${result.value.ext}` : ''))
const fullLink = computed(() => (link.value ? new URL(link.value, window.location.origin).href : ''))

const handleFile = (file: File | undefined) => {
  if (!file) return
  uploading.value = true
  api
    .upload(file)
    .then((uploaded) => {
      result.value = uploaded.data
    })
    .finally(() => {
      uploading.value = false
    })
}

const onUpload = (event: FileUploadUploaderEvent) => {
  handleFile(Array.isArray(event.files) ? event.files[0] : event.files)
}

const onDrop = (event: DragEvent) => {
  dragging.value = false
  handleFile(event.dataTransfer?.files?.[0])
}

const rehost = () => {
  if (!urlInput.value) return
  if (!URL.canParse(urlInput.value)) {
    showToast('', t('upload.invalid_url'), 'error')
    return
  }
  uploading.value = true
  api
    .uploadUrl({ url: urlInput.value })
    .then((uploaded) => {
      result.value = uploaded.data
      urlInput.value = ''
    })
    .finally(() => {
      uploading.value = false
    })
}

const copyLink = () => {
  navigator.clipboard.writeText(fullLink.value)
  showToast('', t('upload.copied'), 'success')
}
</script>

<style scoped>
.upload-wrap {
  max-width: 600px;
  margin: 40px auto;
  padding: 0 16px;
  display: flex;
  flex-direction: column;
  gap: 20px;
}
.dropzone {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 32px;
  border: 2px dashed var(--p-content-border-color);
  border-radius: 8px;
  transition:
    border-color 0.15s,
    background-color 0.15s;
}
.dropzone.dragging {
  border-color: var(--p-primary-color);
  background-color: var(--p-content-hover-background);
}
.drop-hint {
  color: var(--p-text-muted-color);
  font-size: 0.875rem;
}
.url-row {
  display: flex;
  gap: 8px;
}
.result {
  display: flex;
  flex-direction: column;
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
