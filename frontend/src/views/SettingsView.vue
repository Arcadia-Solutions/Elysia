<template>
  <div class="settings-wrap">
    <h2>{{ $t('settings.title') }}</h2>
    <p class="intro">{{ $t('settings.intro') }}</p>

    <div class="field">
      <label>{{ $t('settings.max_file_size.label') }}</label>
      <small>{{ $t('settings.max_file_size.help') }}</small>
      <div class="size-row">
        <InputNumber v-model="fileSize.state.value" size="small" :min="0" fluid :invalid="!!errors.max_file_size_bytes" />
        <Select
          v-model="fileSize.state.unit"
          size="small"
          :options="[
            { label: 'B', value: 1 },
            { label: 'KB', value: 1024 },
            { label: 'MB', value: 1048576 },
            { label: 'GB', value: 1073741824 },
          ]"
          option-label="label"
          option-value="value"
        />
      </div>
      <Message v-if="errors.max_file_size_bytes" severity="error" size="small" variant="simple">{{ errors.max_file_size_bytes }}</Message>
    </div>

    <div class="field">
      <label>{{ $t('settings.max_width.label') }}</label>
      <small>{{ $t('settings.max_width.help') }}</small>
      <InputNumber v-model="form.max_width_pixels" size="small" :min="0" suffix=" px" fluid :invalid="!!errors.max_width_pixels" />
      <Message v-if="errors.max_width_pixels" severity="error" size="small" variant="simple">{{ errors.max_width_pixels }}</Message>
    </div>

    <div class="field">
      <label>{{ $t('settings.max_height.label') }}</label>
      <small>{{ $t('settings.max_height.help') }}</small>
      <InputNumber v-model="form.max_height_pixels" size="small" :min="0" suffix=" px" fluid :invalid="!!errors.max_height_pixels" />
      <Message v-if="errors.max_height_pixels" severity="error" size="small" variant="simple">{{ errors.max_height_pixels }}</Message>
    </div>

    <hr />

    <div class="field">
      <label>{{ $t('settings.target_format.label') }}</label>
      <small>{{ $t('settings.target_format.help') }}</small>
      <Select
        v-model="targetFormat"
        size="small"
        fluid
        :options="[
          { label: $t('settings.target_format.store_as_is'), value: 'store_as_is' },
          { label: 'WebP', value: 'webp' },
          { label: 'JPEG XL', value: 'jpegxl' },
          { label: 'AVIF', value: 'avif' },
          { label: 'PNG', value: 'png' },
          { label: 'JPEG', value: 'jpg' },
        ]"
        option-label="label"
        option-value="value"
      />
    </div>

    <div class="field">
      <label>{{ $t('settings.allow_overriding_file_format.label') }}</label>
      <small>{{ $t('settings.allow_overriding_file_format.help') }}</small>
      <ToggleSwitch v-model="form.allow_overriding_file_format" />
    </div>

    <div
      class="section"
      :class="{ disabled: !form.default_target_file_format }"
      v-tooltip="!form.default_target_file_format ? { value: $t('settings.disabled_reason'), showDelay: 150 } : undefined"
    >
      <div class="field">
        <label>{{ $t('settings.target_width.label') }}</label>
        <small>{{ $t('settings.target_width.help') }}</small>
        <InputNumber v-model="form.target_width_pixels" size="small" :min="0" suffix=" px" fluid :disabled="!form.default_target_file_format" />
      </div>

      <div class="field">
        <label>{{ $t('settings.target_height.label') }}</label>
        <small>{{ $t('settings.target_height.help') }}</small>
        <InputNumber v-model="form.target_height_pixels" size="small" :min="0" suffix=" px" fluid :disabled="!form.default_target_file_format" />
      </div>

      <div class="field">
        <label>{{ $t('settings.target_file_size.label') }}</label>
        <small>{{ $t('settings.target_file_size.help') }}</small>
        <div class="size-row">
          <InputNumber v-model="targetSize.state.value" size="small" :min="0" fluid :disabled="!form.default_target_file_format" />
          <Select
            v-model="targetSize.state.unit"
            size="small"
            :disabled="!form.default_target_file_format"
            :options="[
              { label: 'B', value: 1 },
              { label: 'KB', value: 1024 },
              { label: 'MB', value: 1048576 },
              { label: 'GB', value: 1073741824 },
            ]"
            option-label="label"
            option-value="value"
          />
        </div>
      </div>

      <div class="field">
        <label>{{ $t('settings.default_compression.label') }}</label>
        <small>{{ $t('settings.default_compression.help') }}</small>
        <InputNumber v-model="form.default_compression" size="small" :min="0" :max="100" fluid :disabled="!form.default_target_file_format" />
      </div>

      <div
        class="field"
        v-tooltip="
          compressionOverrideForced && form.default_target_file_format
            ? { value: $t('settings.allow_overriding_compression.forced_help'), showDelay: 150 }
            : undefined
        "
      >
        <label>{{ $t('settings.allow_overriding_compression.label') }}</label>
        <small>{{
          compressionOverrideForced ? $t('settings.allow_overriding_compression.forced_help') : $t('settings.allow_overriding_compression.help')
        }}</small>
        <ToggleSwitch v-model="form.allow_overriding_compression" :disabled="!form.default_target_file_format || compressionOverrideForced" />
      </div>
    </div>

    <hr />

    <div class="field">
      <label>{{ $t('settings.strip_exif_by_default.label') }}</label>
      <small>{{ $t('settings.strip_exif_by_default.help') }}</small>
      <ToggleSwitch v-model="form.strip_exif_by_default" />
    </div>

    <div class="field">
      <label>{{ $t('settings.allow_overriding_strip_exif.label') }}</label>
      <small>{{ $t('settings.allow_overriding_strip_exif.help') }}</small>
      <ToggleSwitch v-model="form.allow_overriding_strip_exif" />
    </div>

    <hr />

    <p class="intro">{{ $t('settings.thumbnails_intro') }}</p>

    <div class="field">
      <label>{{ $t('settings.thumbnail_width.label') }}</label>
      <small>{{ $t('settings.thumbnail_width.help') }}</small>
      <InputNumber v-model="form.thumbnail_width_pixels" size="small" :min="0" suffix=" px" fluid />
    </div>

    <div class="field">
      <label>{{ $t('settings.thumbnail_height.label') }}</label>
      <small>{{ $t('settings.thumbnail_height.help') }}</small>
      <InputNumber v-model="form.thumbnail_height_pixels" size="small" :min="0" suffix=" px" fluid />
    </div>

    <div class="field" :class="{ disabled: !thumbnailsEnabled }">
      <label>{{ $t('settings.thumbnail_quality.label') }}</label>
      <small>{{ $t('settings.thumbnail_quality.help') }}</small>
      <InputNumber
        v-model="form.thumbnail_quality"
        size="small"
        :min="1"
        :max="100"
        fluid
        :disabled="!thumbnailsEnabled"
        :invalid="!!errors.thumbnail_quality"
      />
      <Message v-if="errors.thumbnail_quality" severity="error" size="small" variant="simple">{{ errors.thumbnail_quality }}</Message>
    </div>

    <Button :label="$t('settings.save')" size="small" :loading="saving" :disabled="loading || Object.keys(errors).length > 0" @click="save" />
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import InputNumber from 'primevue/inputnumber'
import Select from 'primevue/select'
import Button from 'primevue/button'
import Message from 'primevue/message'
import ToggleSwitch from 'primevue/toggleswitch'
import Tooltip from 'primevue/tooltip'
import { useI18n } from 'vue-i18n'
import { api } from '@/services/api/http'
import type { ElysiaSettings, TargetFormat } from '@/services/api-schema'
import { showToast } from '@/services/toast'

const vTooltip = Tooltip

const { t } = useI18n()
const loading = ref(true)
const saving = ref(false)

const form = reactive<Required<ElysiaSettings>>({
  max_file_size_bytes: 0,
  max_width_pixels: 0,
  max_height_pixels: 0,
  target_width_pixels: 0,
  target_height_pixels: 0,
  default_target_file_format: null,
  allow_overriding_file_format: false,
  target_file_size_bytes: 0,
  default_compression: 0,
  allow_overriding_compression: true,
  strip_exif_by_default: false,
  allow_overriding_strip_exif: true,
  thumbnail_width_pixels: 0,
  thumbnail_height_pixels: 0,
  thumbnail_quality: 80,
})

// Thumbnails are on when a box is set on either axis.
const thumbnailsEnabled = computed(() => form.thumbnail_width_pixels > 0 || form.thumbnail_height_pixels > 0)

// A byte count edited as value + unit. `bytes` is the canonical count sent to
// the API; `set` shows a stored count as the largest unit that divides it
// evenly (10485760 as "10 MB"), keeping the current unit when the count is 0.
const useByteField = () => {
  const state = reactive({ value: 0, unit: 1048576 })
  const bytes = computed(() => Math.round(state.value * state.unit))
  const set = (count: number) => {
    for (const unit of [1073741824, 1048576, 1024]) {
      if (count >= unit && count % unit === 0) {
        state.value = count / unit
        state.unit = unit
        return
      }
    }
    state.value = count
    if (count !== 0) state.unit = 1
  }
  return { state, bytes, set }
}

const fileSize = useByteField()
const targetSize = useByteField()

// PrimeVue Select shows the placeholder (blank) for a null model, so "store
// as-is" (null) would never appear selected. Map it to a sentinel for the Select.
const targetFormat = computed({
  get: () => form.default_target_file_format ?? 'store_as_is',
  set: (v: TargetFormat | 'store_as_is') => (form.default_target_file_format = v === 'store_as_is' ? null : v),
})

watch(fileSize.bytes, (value) => (form.max_file_size_bytes = value))
watch(targetSize.bytes, (value) => (form.target_file_size_bytes = value))

// AVIF and JPEG have no lossless mode, so if one can be the output there must be
// a quality source. With no default compression, per-upload overriding is the
// only source, so it cannot be turned off.
const losslessIncapableReachable = computed(() => form.allow_overriding_file_format || ['avif', 'jpg'].includes(form.default_target_file_format ?? ''))
const compressionOverrideForced = computed(() => losslessIncapableReachable.value && form.default_compression === 0)
watch(compressionOverrideForced, (forced) => {
  if (forced) form.allow_overriding_compression = true
})

// Mirrors ElysiaSettings::validate on the backend. The "targets set without a
// format" rule is unreachable here (those fields are disabled without a format),
// so it is not duplicated.
const errors = computed(() => {
  const result: Partial<Record<keyof ElysiaSettings, string>> = {}
  if (form.max_file_size_bytes === 0) {
    result.max_file_size_bytes = t('settings.errors.max_file_size_required')
  }
  // A set default format, an allowed override, or thumbnail generation can all
  // trigger a server-side decode, so the pixel caps are required in each case.
  if (form.default_target_file_format || form.allow_overriding_file_format || thumbnailsEnabled.value) {
    if (form.max_width_pixels === 0) result.max_width_pixels = t('settings.errors.max_dimension_required')
    if (form.max_height_pixels === 0) result.max_height_pixels = t('settings.errors.max_dimension_required')
  }
  if (thumbnailsEnabled.value && (form.thumbnail_quality < 1 || form.thumbnail_quality > 100)) {
    result.thumbnail_quality = t('settings.errors.thumbnail_quality_range')
  }
  return result
})

api
  .getElysiaSettings()
  .then((response) => {
    Object.assign(form, response.data)
    fileSize.set(form.max_file_size_bytes)
    targetSize.set(form.target_file_size_bytes)
  })
  .finally(() => {
    loading.value = false
  })

const save = () => {
  if (Object.keys(errors.value).length > 0) return
  saving.value = true
  api
    .putElysiaSettings({ ...form })
    .then((response) => {
      Object.assign(form, response.data)
      fileSize.set(form.max_file_size_bytes)
      targetSize.set(form.target_file_size_bytes)
      showToast('', t('settings.saved'), 'success')
    })
    .finally(() => {
      saving.value = false
    })
}
</script>

<style scoped>
.settings-wrap {
  max-width: 520px;
  margin: 0 auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.settings-wrap h2 {
  margin: 0;
}
.intro {
  margin: 0;
  color: var(--p-text-muted-color, #666);
}
.section {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.section.disabled {
  opacity: 0.5;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.field small {
  color: var(--p-text-muted-color, #666);
}
.size-row {
  display: flex;
  gap: 8px;
}
.size-row > :first-child {
  flex: 1;
}
hr {
  width: 100%;
  border: none;
  border-top: 1px solid var(--p-content-border-color, #ddd);
  margin: 4px 0;
}
</style>
