<template>
  <div class="upload-wrap">
    <div
      v-if="!urlInput"
      class="dropzone"
      :class="{ dragging }"
      @dragover.prevent
      @dragenter.prevent="dragging = true"
      @dragleave.prevent="dragging = false"
      @drop.prevent="onDrop"
    >
      <FileUpload mode="basic" accept="image/*" :auto="false" customUpload :chooseLabel="$t('upload.choose')" @select="onSelect" :disabled="uploading" />
      <span class="drop-hint">{{ $t('upload.drop_hint') }}</span>
    </div>

    <div v-if="selectedPreview" class="preview-wrap">
      <img :src="selectedPreview" :alt="$t('upload.preview_alt')" class="preview" />
      <Button
        icon="pi pi-times"
        size="small"
        rounded
        severity="secondary"
        class="clear-btn"
        :title="$t('upload.clear')"
        :aria-label="$t('upload.clear')"
        :disabled="uploading"
        @click="clearSelection"
      />
    </div>

    <InputText
      v-if="!selectedFile"
      v-model="urlInput"
      size="small"
      :placeholder="$t('upload.url_placeholder')"
      fluid
      @keyup.enter="submit"
      :disabled="uploading"
    />

    <div v-if="settings?.allow_overriding_file_format" class="format-row">
      <label>{{ $t('upload.format_override.label') }}</label>
      <Select
        v-model="formatOverride"
        size="small"
        fluid
        :disabled="uploading"
        :options="[
          { label: $t('upload.format_override.keep_default'), value: 'default' },
          ...Object.values(TargetFormat).map((f) => ({ label: formatLabels[f], value: f })),
        ]"
        option-label="label"
        option-value="value"
      />
    </div>

    <div v-if="showCompression" class="quality-row">
      <label>{{ $t('upload.quality.label', { value: quality }) }}</label>
      <Slider v-model="quality" :min="1" :max="100" :disabled="uploading" />
    </div>

    <div class="upload-row">
      <Button :label="$t('upload.upload')" size="small" @click="submit" :disabled="uploading || (!selectedFile && !urlInput)" />
    </div>

    <Transition name="fade">
      <div v-if="uploading" class="progress">
        <ProgressBar v-if="phase === 'uploading'" :value="progress" :showValue="false" style="height: 6px" />
        <ProgressBar v-else mode="indeterminate" style="height: 6px" />
        <span class="progress-label">
          {{ phase === 'uploading' ? $t('upload.uploading', { pct: progress }) : $t('upload.processing') }}
        </span>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import FileUpload, { type FileUploadSelectEvent } from 'primevue/fileupload'
import InputText from 'primevue/inputtext'
import Button from 'primevue/button'
import ProgressBar from 'primevue/progressbar'
import Select from 'primevue/select'
import Slider from 'primevue/slider'
import { useI18n } from 'vue-i18n'
import { api } from '@/services/api/http'
import { TargetFormat, type PublicElysiaSettings } from '@/services/api-schema'
import { showToast } from '@/services/toast'

const { t } = useI18n()
const router = useRouter()
// 'uploading' = bytes in flight (determinate bar), 'processing' = backend
const phase = ref<'idle' | 'uploading' | 'processing'>('idle')
const progress = ref(0)
const uploading = computed(() => phase.value !== 'idle')
const dragging = ref(false)
const urlInput = ref('')
// File chosen but not yet uploaded, with an object URL for the preview.
const selectedFile = ref<File | null>(null)
const selectedPreview = ref('')
const settings = ref<PublicElysiaSettings | null>(null)
// Per-upload output format; 'default' uses the server's default (a non-empty
// sentinel so PrimeVue shows it as selected). Only honored when the settings
// allow overriding; the backend rejects it otherwise.
const formatOverride = ref<TargetFormat | 'default'>('default')
const formatLabels: Record<TargetFormat, string> = {
  webp: 'WebP',
  jpegxl: 'JPEG XL',
  avif: 'AVIF',
  png: 'PNG',
  jpg: 'JPEG',
}
// Per-upload lossy quality, sent only while the slider is shown.
const quality = ref(80)

// The format that will actually be applied: the override, or the server default.
const effectiveFormat = computed(() => (formatOverride.value === 'default' ? settings.value?.default_target_file_format : formatOverride.value))
// Quality applies only to lossy-capable formats (every target format but PNG),
// and only when the settings allow overriding compression.
const showCompression = computed(() => !!settings.value?.allow_overriding_compression && !!effectiveFormat.value && effectiveFormat.value !== 'png')
const qualityValue = () => (showCompression.value ? quality.value : undefined)

// Public upload limits, so files are rejected client-side before a wasted
// round-trip. The backend still enforces them. A 0 limit means "no limit".
onMounted(() => {
  api.getPublicElysiaSettings().then((r) => {
    settings.value = r.data
  })
})

const formatBytes = (n: number): string => {
  const units = ['B', 'KB', 'MB', 'GB']
  let i = 0
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024
    i++
  }
  return `${Math.round(n * 10) / 10} ${units[i]}`
}

const readDimensions = (file: File): Promise<{ width: number; height: number }> =>
  new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    const img = new Image()
    img.onload = () => {
      URL.revokeObjectURL(url)
      resolve({ width: img.naturalWidth, height: img.naturalHeight })
    }
    img.onerror = () => {
      URL.revokeObjectURL(url)
      reject()
    }
    img.src = url
  })

// Resolves to an error message when the file breaks a limit, else null.
const validate = (file: File): Promise<string | null> => {
  const s = settings.value
  if (!s) return Promise.resolve(null)
  if (s.max_file_size_bytes > 0 && file.size > s.max_file_size_bytes) {
    return Promise.resolve(t('upload.too_large', { max: formatBytes(s.max_file_size_bytes) }))
  }
  if (s.max_width_pixels === 0 && s.max_height_pixels === 0) return Promise.resolve(null)
  return readDimensions(file)
    .then(({ width, height }) => {
      if (s.max_width_pixels > 0 && width > s.max_width_pixels) {
        return t('upload.too_wide', { max: s.max_width_pixels })
      }
      if (s.max_height_pixels > 0 && height > s.max_height_pixels) {
        return t('upload.too_tall', { max: s.max_height_pixels })
      }
      return null
    })
    .catch(() => null) // undecodable: let the backend judge it
}

const handleFile = (file: File | undefined) => {
  if (!file) return
  validate(file).then((error) => {
    if (error) {
      showToast('', error, 'error')
      return
    }
    phase.value = 'uploading'
    progress.value = 0
    api
      .upload(file, qualityValue(), formatOverride.value === 'default' ? undefined : formatOverride.value, {
        onUploadProgress: (event) => {
          if (!event.total) return
          progress.value = Math.round((event.loaded / event.total) * 100)
          // Bytes all sent: backend is now processing, switch to indeterminate.
          if (progress.value >= 100) phase.value = 'processing'
        },
      })
      .then((uploaded) => {
        router.push({ name: 'Uploaded', query: { id: uploaded.data.id, ext: uploaded.data.ext } })
      })
      .finally(() => {
        phase.value = 'idle'
        clearSelection()
      })
  })
}

const clearSelection = () => {
  if (selectedPreview.value) URL.revokeObjectURL(selectedPreview.value)
  selectedFile.value = null
  selectedPreview.value = ''
}

const pick = (file: File | undefined) => {
  if (!file) return
  clearSelection()
  selectedFile.value = file
  selectedPreview.value = URL.createObjectURL(file)
}

const onSelect = (event: FileUploadSelectEvent) => {
  pick(Array.isArray(event.files) ? event.files[0] : event.files)
}

const onDrop = (event: DragEvent) => {
  dragging.value = false
  pick(event.dataTransfer?.files?.[0])
}

// One button: upload the chosen file, else rehost the pasted URL.
const submit = () => {
  if (selectedFile.value) return handleFile(selectedFile.value)
  rehost()
}

const rehost = () => {
  if (!urlInput.value) return
  if (!URL.canParse(urlInput.value)) {
    showToast('', t('upload.invalid_url'), 'error')
    return
  }
  // Rehost sends only the URL; the backend fetches and processes, so there is
  // no client-side transfer to track, just the processing phase.
  phase.value = 'processing'
  api
    .uploadUrl({ url: urlInput.value }, qualityValue(), formatOverride.value === 'default' ? undefined : formatOverride.value)
    .then((uploaded) => {
      router.push({ name: 'Uploaded', query: { id: uploaded.data.id, ext: uploaded.data.ext } })
    })
    .finally(() => {
      phase.value = 'idle'
    })
}
</script>

<style scoped>
.upload-wrap {
  max-width: 600px;
  margin: 0 auto;
  padding: 0 16px;
  min-height: 80vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
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
.upload-row {
  margin-top: 30px;
  display: flex;
  justify-content: center;
}
.preview-wrap {
  position: relative;
}
.clear-btn {
  position: absolute;
  top: 8px;
  right: 8px;
}
.format-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.format-row label {
  color: var(--p-text-muted-color);
  font-size: 0.875rem;
}
.quality-row {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 0 4px;
}
.quality-row label {
  color: var(--p-text-muted-color);
  font-size: 0.875rem;
}
.progress {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.progress :deep(.p-progressbar-value) {
  transition: width 0.3s ease;
}
.progress-label {
  color: var(--p-text-muted-color);
  font-size: 0.875rem;
  text-align: center;
}
.fade-enter-active,
.fade-leave-active {
  transition:
    opacity 0.25s ease,
    transform 0.25s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
.preview {
  max-width: 100%;
  border-radius: 6px;
}
</style>
