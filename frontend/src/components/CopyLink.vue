<template>
  <div class="link-field">
    <label>{{ label }}</label>
    <div class="link-row">
      <InputText :value="fullLink" size="small" readonly fluid />
      <Button icon="pi pi-copy" size="small" :title="$t('upload.copy')" :aria-label="$t('upload.copy')" @click="copyLink" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import InputText from 'primevue/inputtext'
import Button from 'primevue/button'
import { useI18n } from 'vue-i18n'
import { showToast } from '@/services/toast'

// A labelled, read-only absolute link with a copy button. `link` may be relative.
const props = defineProps<{ label: string; link: string }>()

const { t } = useI18n()

const fullLink = computed(() => (props.link ? new URL(props.link, window.location.origin).href : ''))

const copyLink = () => {
  navigator.clipboard
    .writeText(fullLink.value)
    .then(() => showToast(t('upload.copied'), 'success'))
    .catch(() => showToast(t('upload.copy_failed'), 'error'))
}
</script>

<style scoped>
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
</style>
