<template>
  <div class="settings-wrap" @click="toggleSection">
    <h2>{{ $t('settings.title') }}</h2>
    <p class="intro">{{ $t('settings.intro') }}</p>

    <Panel :header="$t('settings.sections.limits')" toggleable collapsed>
      <div class="field">
        <label>{{ $t('settings.max_file_size.label') }}</label>
        <small>{{ $t('settings.max_file_size.help') }}</small>
        <ByteInput v-model="form.max_file_size_bytes" :invalid="!!errors.max_file_size_bytes" />
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
    </Panel>

    <Panel :header="$t('settings.sections.conversion')" toggleable collapsed>
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
          <ByteInput v-model="form.target_file_size_bytes" :disabled="!form.default_target_file_format" />
        </div>

        <div class="field">
          <label>{{ $t('settings.default_compression.label') }}</label>
          <small>{{ $t('settings.default_compression.help') }}</small>
          <InputNumber
            v-model="form.default_compression"
            size="small"
            :min="0"
            :max="100"
            fluid
            :disabled="!form.default_target_file_format"
            :invalid="!!errors.default_compression"
          />
          <Message v-if="errors.default_compression" severity="error" size="small" variant="simple">{{ errors.default_compression }}</Message>
        </div>

        <div
          class="field"
          v-tooltip="
            form.default_target_file_format && !losslessAllowed
              ? {
                  value: form.default_compression === 0 ? $t('settings.allow_lossless.already_lossless') : $t('settings.allow_lossless.format_unsupported'),
                  showDelay: 150,
                }
              : undefined
          "
        >
          <label>{{ $t('settings.allow_lossless.label') }}</label>
          <small>{{ $t('settings.allow_lossless.help') }}</small>
          <ToggleSwitch v-model="form.allow_lossless" :disabled="!losslessAllowed" />
        </div>
      </div>
    </Panel>

    <Panel :header="$t('settings.sections.exif')" toggleable collapsed>
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
    </Panel>

    <Panel :header="$t('settings.sections.expiry')" toggleable collapsed>
      <div class="field">
        <label>{{ $t('settings.default_expiry_days.label') }}</label>
        <small>{{ $t('settings.default_expiry_days.help') }}</small>
        <InputNumber v-model="form.default_expiry_days" size="small" :min="0" :max="MAX_EXPIRY_DAYS" fluid />
      </div>

      <div class="field">
        <label>{{ $t('settings.allow_overriding_expiry.label') }}</label>
        <small>{{ $t('settings.allow_overriding_expiry.help') }}</small>
        <ToggleSwitch v-model="form.allow_overriding_expiry" />
      </div>
    </Panel>

    <Panel :header="$t('settings.sections.uploads')" toggleable collapsed>
      <div class="field">
        <label>{{ $t('settings.uploads_enabled.label') }}</label>
        <small>{{ $t('settings.uploads_enabled.help') }}</small>
        <ToggleSwitch v-model="form.uploads_enabled" />
      </div>

      <div v-if="!form.uploads_enabled" class="field">
        <label>{{ $t('settings.uploads_disabled_message.label') }}</label>
        <small>{{ $t('settings.uploads_disabled_message.help') }}</small>
        <Textarea v-model="form.uploads_disabled_message" size="small" rows="3" autoResize fluid />
      </div>

      <div class="field">
        <label>{{ $t('settings.url_uploads.label') }}</label>
        <small>{{ $t('settings.url_uploads.help') }}</small>
        <Select
          v-model="form.url_uploads"
          size="small"
          fluid
          :options="[
            { label: $t('settings.url_uploads.everyone'), value: 'everyone' },
            { label: $t('settings.url_uploads.sso_only'), value: 'sso_only' },
            { label: $t('settings.url_uploads.disabled'), value: 'disabled' },
          ]"
          option-label="label"
          option-value="value"
        />
      </div>

      <div class="field">
        <label>{{ $t('settings.albums_enabled.label') }}</label>
        <small>{{ $t('settings.albums_enabled.help') }}</small>
        <ToggleSwitch v-model="form.albums_enabled" />
      </div>

      <div class="field">
        <label>{{ $t('settings.checksum_algorithms.label') }}</label>
        <small>{{ $t('settings.checksum_algorithms.help') }}</small>
        <MultiSelect
          v-model="form.checksum_algorithms"
          size="small"
          fluid
          :options="
            Object.values(ChecksumAlgorithm)
              .filter((algorithm) => algorithm !== 'sha256')
              .map((value) => ({ label: $t(`checksums.${value}`), value }))
          "
          filter
          option-label="label"
          option-value="value"
        />
      </div>
    </Panel>

    <Panel :header="$t('settings.sections.accounts')" toggleable collapsed>
      <p class="intro">{{ $t('settings.accounts_intro') }}</p>

      <div class="field">
        <label>{{ $t('settings.allow_registration.label') }}</label>
        <small>{{ $t('settings.allow_registration.help') }}</small>
        <ToggleSwitch v-model="form.allow_registration" />
      </div>

      <div class="field">
        <label>{{ $t('settings.default_storage_quota.label') }}</label>
        <small>{{ $t('settings.default_storage_quota.help') }}</small>
        <ByteInput v-model="form.default_storage_quota_bytes" />
      </div>
    </Panel>

    <Panel :header="$t('settings.sections.referrers')" toggleable collapsed>
      <div class="field">
        <label>{{ $t('settings.allowed_referrers.label') }}</label>
        <small>{{ $t('settings.allowed_referrers.help') }}</small>
        <AutoComplete v-model="form.allowed_referrers" size="small" multiple :typeahead="false" fluid placeholder="example.com" />
      </div>

      <div class="field">
        <label>{{ $t('settings.record_referrer_views.label') }}</label>
        <small>{{ $t('settings.record_referrer_views.help') }}</small>
        <ToggleSwitch v-model="form.record_referrer_views" />
      </div>
    </Panel>

    <Panel :header="$t('settings.sections.medium')" toggleable collapsed>
      <p class="intro">{{ $t('settings.medium_intro') }}</p>

      <div class="field">
        <label>{{ $t('settings.medium_width.label') }}</label>
        <small>{{ $t('settings.medium_width.help') }}</small>
        <InputNumber v-model="form.medium_width_pixels" size="small" :min="0" suffix=" px" fluid />
      </div>

      <div class="field">
        <label>{{ $t('settings.medium_height.label') }}</label>
        <small>{{ $t('settings.medium_height.help') }}</small>
        <InputNumber v-model="form.medium_height_pixels" size="small" :min="0" suffix=" px" fluid />
      </div>

      <div class="field" :class="{ disabled: !mediumEnabled }">
        <label>{{ $t('settings.medium_quality.label') }}</label>
        <small>{{ $t('settings.medium_quality.help') }}</small>
        <InputNumber v-model="form.medium_quality" size="small" :min="1" :max="100" fluid :disabled="!mediumEnabled" :invalid="!!errors.medium_quality" />
        <Message v-if="errors.medium_quality" severity="error" size="small" variant="simple">{{ errors.medium_quality }}</Message>
      </div>
    </Panel>

    <Panel :header="$t('settings.sections.thumbnail')" toggleable collapsed>
      <p class="intro">{{ $t('settings.thumbnail_intro') }}</p>

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

      <div class="field" :class="{ disabled: !thumbnailEnabled }">
        <label>{{ $t('settings.thumbnail_quality.label') }}</label>
        <small>{{ $t('settings.thumbnail_quality.help') }}</small>
        <InputNumber
          v-model="form.thumbnail_quality"
          size="small"
          :min="1"
          :max="100"
          fluid
          :disabled="!thumbnailEnabled"
          :invalid="!!errors.thumbnail_quality"
        />
        <Message v-if="errors.thumbnail_quality" severity="error" size="small" variant="simple">{{ errors.thumbnail_quality }}</Message>
      </div>
    </Panel>

    <Panel :header="$t('settings.sections.legal')" toggleable collapsed>
      <p class="intro">{{ $t('settings.legal_intro') }}</p>

      <div class="field">
        <label>{{ $t('settings.terms_of_service.label') }}</label>
        <Textarea v-model="form.terms_of_service" size="small" rows="8" autoResize fluid />
      </div>

      <div class="field">
        <label>{{ $t('settings.privacy_policy.label') }}</label>
        <Textarea v-model="form.privacy_policy" size="small" rows="8" autoResize fluid />
      </div>
    </Panel>

    <Button :label="$t('settings.save')" size="small" :loading="saving" :disabled="loading || Object.keys(errors).length > 0" @click="save" />
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { MAX_EXPIRY_DAYS } from '@/services/format'
import AutoComplete from 'primevue/autocomplete'
import InputNumber from 'primevue/inputnumber'
import MultiSelect from 'primevue/multiselect'
import Select from 'primevue/select'
import Textarea from 'primevue/textarea'
import Button from 'primevue/button'
import Message from 'primevue/message'
import Panel from 'primevue/panel'
import ToggleSwitch from 'primevue/toggleswitch'
import Tooltip from 'primevue/tooltip'
import { useI18n } from 'vue-i18n'
import ByteInput from '@/components/ByteInput.vue'
import { api } from '@/services/api/http'
import { ChecksumAlgorithm, type ElysiaSettings, type TargetFormat } from '@/services/api-schema'
import { showToast } from '@/services/toast'

const vTooltip = Tooltip

// Clicking anywhere on a panel header toggles it, not only the toggle button.
const toggleSection = (e: MouseEvent) => {
  const target = e.target as HTMLElement
  if (target.closest('button')) return
  target.closest('.p-panel-header')?.querySelector('button')?.click()
}

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
  target_file_size_bytes: 0,
  default_compression: 0,
  allow_lossless: false,
  strip_exif_by_default: false,
  allow_overriding_strip_exif: true,
  medium_width_pixels: 0,
  medium_height_pixels: 0,
  medium_quality: 80,
  thumbnail_width_pixels: 0,
  thumbnail_height_pixels: 0,
  thumbnail_quality: 80,
  allow_registration: true,
  allowed_referrers: [],
  uploads_enabled: true,
  default_storage_quota_bytes: 0,
  record_referrer_views: false,
  terms_of_service: '',
  privacy_policy: '',
  default_expiry_days: 0,
  allow_overriding_expiry: true,
  albums_enabled: true,
  url_uploads: 'everyone',
  uploads_disabled_message: '',
  checksum_algorithms: [],
})

// Medium images are on when a box is set on either axis.
const mediumEnabled = computed(() => form.medium_width_pixels > 0 || form.medium_height_pixels > 0)
// Thumbnails are cropped to the box, so they need both axes.
const thumbnailEnabled = computed(() => form.thumbnail_width_pixels > 0 && form.thumbnail_height_pixels > 0)

// PrimeVue Select shows the placeholder (blank) for a null model, so "store
// as-is" (null) would never appear selected. Map it to a sentinel for the Select.
const targetFormat = computed({
  get: () => form.default_target_file_format ?? 'store_as_is',
  set: (v: TargetFormat | 'store_as_is') => (form.default_target_file_format = v === 'store_as_is' ? null : v),
})

// Mirrors TargetFormat::supports_lossless on the backend, minus PNG (never lossy).
const lossyAndLosslessFormats = ['webp', 'jpegxl']
// Lossless is only an alternative to a lossy default, in a format that has it.
const losslessAllowed = computed(() => form.default_compression > 0 && lossyAndLosslessFormats.includes(form.default_target_file_format ?? ''))
watch(losslessAllowed, (allowed) => {
  if (!allowed) form.allow_lossless = false
})

// Mirrors ElysiaSettings::validate on the backend. The "targets set without a
// format" rule is unreachable here (those fields are disabled without a format),
// so it is not duplicated.
const errors = computed(() => {
  const result: Partial<Record<keyof ElysiaSettings, string>> = {}
  if (form.max_file_size_bytes === 0) {
    result.max_file_size_bytes = t('settings.errors.max_file_size_required')
  }
  // A set default format or preview generation can trigger a server-side
  // decode, so the pixel caps are required in each case.
  if (form.default_target_file_format || mediumEnabled.value || thumbnailEnabled.value) {
    if (form.max_width_pixels === 0) result.max_width_pixels = t('settings.errors.max_dimension_required')
    if (form.max_height_pixels === 0) result.max_height_pixels = t('settings.errors.max_dimension_required')
  }
  if (['avif', 'jpg'].includes(form.default_target_file_format ?? '') && form.default_compression === 0) {
    result.default_compression = t('settings.errors.compression_required')
  }
  if (form.default_target_file_format === 'png' && form.default_compression > 0) {
    result.default_compression = t('settings.errors.png_lossless')
  }
  if (mediumEnabled.value && (form.medium_quality < 1 || form.medium_quality > 100)) {
    result.medium_quality = t('settings.errors.quality_range')
  }
  if (thumbnailEnabled.value && (form.thumbnail_quality < 1 || form.thumbnail_quality > 100)) {
    result.thumbnail_quality = t('settings.errors.quality_range')
  }
  return result
})

api
  .getElysiaSettings()
  .then((response) => {
    Object.assign(form, response.data)
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
      showToast(t('settings.saved'), 'success')
    })
    .finally(() => {
      saving.value = false
    })
}
</script>

<style scoped>
.settings-wrap {
  max-width: 1040px;
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
:deep(.p-panel-header) {
  cursor: pointer;
}
/* Panel content is a plain block; space the fields like the rest of the page. */
:deep(.p-panel-content) {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
</style>
