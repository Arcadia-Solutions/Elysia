<template>
  <div class="upload-wrap">
    <Message v-if="settings && !settings.uploads_enabled" severity="warn" size="small">{{ $t('upload.disabled') }}</Message>
    <template v-else>
      <section class="card">
        <header class="card-head">
          <h2>{{ $t('upload.step.image') }}</h2>
        </header>

        <div v-if="!urlInput" class="dropzone" :class="{ dragging }">
          <FileUpload
            mode="basic"
            accept="image/*"
            multiple
            :auto="false"
            customUpload
            :chooseLabel="$t('upload.choose')"
            @select="onSelect"
            :disabled="uploading"
          >
            <template #filelabel><span hidden /></template>
          </FileUpload>
          <span class="drop-hint">{{ $t('upload.drop_hint') }}</span>
        </div>

        <div v-if="selected.length" class="previews">
          <div v-for="(item, i) in selected" :key="item.preview" class="preview-wrap">
            <img :src="item.preview" :alt="$t('upload.preview_alt')" class="preview" />
            <Button
              icon="pi pi-times"
              size="small"
              rounded
              severity="secondary"
              class="clear-btn"
              :title="$t('upload.clear')"
              :aria-label="$t('upload.clear')"
              :disabled="uploading"
              @click="remove(i)"
            />
            <div v-if="item.status === 'uploading' || item.status === 'processing' || item.status === 'done'" class="item-progress">
              <ProgressBar v-if="item.status === 'processing'" mode="indeterminate" />
              <ProgressBar v-else :value="item.progress" :showValue="false" />
            </div>
          </div>
        </div>

        <InputText
          v-if="!selected.length"
          v-model="urlInput"
          size="small"
          :placeholder="$t('upload.url_placeholder')"
          fluid
          @keyup.enter="submit"
          :disabled="uploading"
        />
      </section>

      <section class="card">
        <header class="card-head">
          <h2>{{ $t('upload.step.options') }}</h2>
        </header>

        <div class="format-row">
          <label>{{ $t('albums.album') }}</label>
          <AlbumSelect v-model="albumId" mine :disabled="uploading" :placeholder="$t('albums.none')" />
        </div>

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

        <div v-if="showLosslessToggle" class="toggle-row">
          <label>{{ $t('upload.lossless.label') }}</label>
          <ToggleSwitch v-model="lossless" :disabled="uploading" />
        </div>

        <div v-if="showCompression && !losslessSelected" class="quality-row">
          <label>{{ $t('upload.quality.label', { value: quality }) }}</label>
          <Slider v-model="quality" :min="1" :max="100" :disabled="uploading" />
        </div>

        <div v-if="settings?.allow_overriding_strip_exif" class="toggle-row">
          <label>{{ $t('upload.strip_exif.label') }}</label>
          <ToggleSwitch v-model="stripExif" :disabled="uploading" />
        </div>

        <div v-if="settings?.allow_overriding_expiry" class="format-row">
          <label>{{ $t('upload.expiry_days.label') }}</label>
          <InputNumber v-model="expiryDays" size="small" :min="0" :max="MAX_EXPIRY_DAYS" fluid :disabled="uploading" />
        </div>
      </section>

      <Button :label="$t('upload.upload')" @click="submit" fluid :disabled="uploading || (!selected.length && !urlInput)" />

      <Transition name="fade">
        <div v-if="phase === 'processing'" class="progress">
          <ProgressBar mode="indeterminate" style="height: 6px" />
          <span class="progress-label">{{ $t('upload.processing') }}</span>
        </div>
      </Transition>
    </template>
  </div>
</template>

<script setup lang="ts">
import { MAX_EXPIRY_DAYS, formatBytes } from '@/services/format'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import FileUpload, { type FileUploadSelectEvent } from 'primevue/fileupload'
import InputNumber from 'primevue/inputnumber'
import InputText from 'primevue/inputtext'
import Message from 'primevue/message'
import Button from 'primevue/button'
import ProgressBar from 'primevue/progressbar'
import Select from 'primevue/select'
import Slider from 'primevue/slider'
import ToggleSwitch from 'primevue/toggleswitch'
import { useI18n } from 'vue-i18n'
import AlbumSelect from '@/components/AlbumSelect.vue'
import { api } from '@/services/api/http'
import { TargetFormat, type PublicElysiaSettings } from '@/services/api-schema'
import { showToast } from '@/services/toast'

const { t } = useI18n()
const router = useRouter()
// 'uploading' = files in flight (each shows its own bar), 'processing' = backend
// fetching a rehosted URL (one page-wide bar)
const phase = ref<'idle' | 'uploading' | 'processing'>('idle')
const uploading = computed(() => phase.value !== 'idle')
const dragging = ref(false)
const urlInput = ref('')
// Files chosen but not yet uploaded, each with an object URL for its preview
// and its upload state: 'uploading' = bytes in flight (progress in %),
// 'processing' = bytes sent, backend working.
type Selected = { file: File; preview: string; status: 'pending' | 'uploading' | 'processing' | 'done' | 'failed'; progress: number }
const selected = ref<Selected[]>([])
// Cached by App.vue on load and on login.
const settings: PublicElysiaSettings | null = JSON.parse(localStorage.getItem('public_settings') ?? 'null')
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
// Per-upload lossless choice, offered only for formats with a lossless mode.
const lossless = ref(false)
// Per-upload EXIF-stripping choice, initialized from the server default.
// Only honored when the settings allow overriding.
const stripExif = ref(!!settings?.strip_exif_by_default)
// Per-upload expiry in days (0 never expires), initialized from the server
// default. Only honored when the settings allow overriding; cleared uses the
// default.
const expiryDays = ref<number | null>(settings?.default_expiry_days ?? 0)
// One of the caller's albums the uploads go into, if any.
const albumId = ref<string | null>(null)

// The format that will actually be applied: the override, or the server default.
const effectiveFormat = computed(() => (formatOverride.value === 'default' ? settings?.default_target_file_format : formatOverride.value))
// Quality applies only to lossy-capable formats (every target format but PNG),
// and only when the settings allow overriding compression.
const showCompression = computed(() => !!settings?.allow_overriding_compression && !!effectiveFormat.value && effectiveFormat.value !== 'png')
// PNG is always lossless and AVIF/JPEG never are, so only WebP and JPEG XL get the toggle.
const showLosslessToggle = computed(() => showCompression.value && ['webp', 'jpegxl'].includes(effectiveFormat.value ?? ''))
const losslessSelected = computed(() => showLosslessToggle.value && lossless.value)
// 0 asks the backend for lossless, overriding its default quality, which PNG refuses.
const qualityValue = () =>
  losslessSelected.value || (settings?.allow_overriding_compression && effectiveFormat.value === 'png') ? 0 : showCompression.value ? quality.value : undefined
const stripExifValue = () => (settings?.allow_overriding_strip_exif ? stripExif.value : undefined)
const expiryDaysValue = () => (settings?.allow_overriding_expiry ? (expiryDays.value ?? undefined) : undefined)
// Puts the uploaded images in the picked album, if any; resolves either way so
// the result page still opens when that fails (the interceptor toasts it).
const addToAlbum = (ids: string[]): Promise<void> =>
  albumId.value && ids.length
    ? api.addAlbumImages(albumId.value, { ids }).then(
        () => undefined,
        () => undefined,
      )
    : Promise.resolve()

onMounted(() => {
  window.addEventListener('paste', onPaste)
  window.addEventListener('dragover', onDragOver)
  window.addEventListener('dragleave', onDragLeave)
  window.addEventListener('drop', onDrop)
})
onUnmounted(() => {
  window.removeEventListener('paste', onPaste)
  window.removeEventListener('dragover', onDragOver)
  window.removeEventListener('dragleave', onDragLeave)
  window.removeEventListener('drop', onDrop)
})

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

// Public upload limits, so files are rejected client-side before a wasted
// round-trip. The backend still enforces them. A 0 limit means "no limit".
// Resolves to an error message when the file breaks a limit, else null.
const validate = (file: File): Promise<string | null> => {
  const s = settings
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

// Uploads the files one after another so the backend sees a single upload at
// a time. A failed file (toasted by the http interceptor) is skipped; the
// result page lists the ones that made it.
const uploadAll = (items: Selected[]) => {
  const ids: string[] = []
  const urls: string[] = []
  const thumbnailUrls: string[] = []
  phase.value = 'uploading'
  items
    .reduce(
      (chain, item) =>
        chain.then(() => {
          item.status = 'uploading'
          return api
            .upload(item.file, qualityValue(), formatOverride.value === 'default' ? undefined : formatOverride.value, stripExifValue(), expiryDaysValue(), {
              onUploadProgress: (event) => {
                item.progress = event.total ? Math.round((event.loaded / event.total) * 100) : 0
                // Bytes all sent: backend is now processing, switch to indeterminate.
                if (item.progress >= 100) item.status = 'processing'
              },
            })
            .then((uploaded) => {
              item.status = 'done'
              item.progress = 100
              ids.push(uploaded.data.id)
              urls.push(uploaded.data.url)
              thumbnailUrls.push(uploaded.data.thumbnail_url)
            })
            .catch(() => {
              item.status = 'failed'
            })
        }),
      Promise.resolve(),
    )
    .then(() => addToAlbum(ids))
    .then(() => {
      if (urls.length) router.push({ name: 'Uploaded', query: { id: ids, url: urls, thumbnailUrl: thumbnailUrls } })
    })
    .finally(() => {
      phase.value = 'idle'
      clearSelection()
    })
}

const clearSelection = () => {
  selected.value.forEach((item) => URL.revokeObjectURL(item.preview))
  selected.value = []
}

const remove = (index: number) => {
  URL.revokeObjectURL(selected.value[index].preview)
  selected.value.splice(index, 1)
}

// Adds the files that pass the limits, in the order given, toasting the
// reason for each rejected one.
const pick = (files: File[]) => {
  Promise.all(files.map(validate)).then((errors) =>
    files.forEach((file, i) => {
      if (errors[i]) return showToast(`${file.name}: ${errors[i]}`, 'error')
      selected.value.push({ file, preview: URL.createObjectURL(file), status: 'pending', progress: 0 })
    }),
  )
}

// Drops and pastes skip the file input's `accept` check, so filter them here.
const imageFiles = (files: FileList | undefined) => Array.from(files ?? []).filter((file) => file.type.startsWith('image/'))

const onSelect = (event: FileUploadSelectEvent) => {
  pick(event.files)
}

// Page-wide drop, like paste. Only file drags are claimed, so dragging text
// into the URL field still works; claiming them also stops the browser from
// navigating away to a file dropped outside the drop zone.
const canAdd = () => !uploading.value && settings?.uploads_enabled !== false
const onDragOver = (event: DragEvent) => {
  if (!event.dataTransfer?.types.includes('Files')) return
  event.preventDefault()
  dragging.value = canAdd()
}
// A null relatedTarget means the pointer left the window.
const onDragLeave = (event: DragEvent) => {
  if (!event.relatedTarget) dragging.value = false
}
const onDrop = (event: DragEvent) => {
  if (!event.dataTransfer?.types.includes('Files')) return
  event.preventDefault()
  dragging.value = false
  if (!canAdd()) return
  urlInput.value = ''
  pick(imageFiles(event.dataTransfer.files))
}

// Page-wide paste: grab a pasted image file, leave text pastes (e.g. into the URL field) alone.
const onPaste = (event: ClipboardEvent) => {
  const files = imageFiles(event.clipboardData?.files)
  if (!canAdd() || !files.length) return
  event.preventDefault()
  urlInput.value = ''
  pick(files)
}

// One button: upload the chosen files, else rehost the pasted URL.
const submit = () => {
  if (selected.value.length) return uploadAll(selected.value)
  rehost()
}

const rehost = () => {
  if (!urlInput.value) return
  if (!URL.canParse(urlInput.value)) {
    showToast(t('upload.invalid_url'), 'error')
    return
  }
  // Rehost sends only the URL; the backend fetches and processes, so there is
  // no client-side transfer to track, just the processing phase.
  phase.value = 'processing'
  api
    .uploadUrl(
      { url: urlInput.value },
      qualityValue(),
      formatOverride.value === 'default' ? undefined : formatOverride.value,
      stripExifValue(),
      expiryDaysValue(),
    )
    .then((uploaded) => addToAlbum([uploaded.data.id]).then(() => uploaded))
    .then((uploaded) => {
      router.push({ name: 'Uploaded', query: { id: uploaded.data.id, url: uploaded.data.url, thumbnailUrl: uploaded.data.thumbnail_url } })
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
  padding: 24px 16px;
  min-height: 80vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 16px;
}
.card {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 20px;
  border: 2px solid var(--p-content-border-color);
  border-radius: 10px;
  background: var(--p-content-background);
}
.card-head {
  display: flex;
  align-items: center;
  gap: 10px;
}
.card-head h2 {
  margin: 0;
  font-size: 0.95rem;
  font-weight: 600;
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
.previews {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 8px;
  align-items: center;
}
.previews:has(> :only-child) {
  grid-template-columns: 1fr;
}
.preview-wrap {
  position: relative;
}
.item-progress {
  position: absolute;
  left: 8px;
  right: 8px;
  top: 50%;
  transform: translateY(-50%);
}
.item-progress :deep(.p-progressbar) {
  height: 6px;
}
.item-progress :deep(.p-progressbar-value) {
  transition: width 0.3s ease;
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
.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.toggle-row label {
  color: var(--p-text-muted-color);
  font-size: 0.875rem;
}
.progress {
  display: flex;
  flex-direction: column;
  gap: 8px;
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
  display: block;
  width: 100%;
  border-radius: 6px;
}
</style>
