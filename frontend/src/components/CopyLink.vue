<template>
  <InputGroup>
    <InputGroupAddon class="label">{{ label }}</InputGroupAddon>
    <InputText :value="value" size="small" readonly fluid />
    <Button icon="pi pi-copy" size="small" :title="$t('upload.copy')" :aria-label="$t('upload.copy')" @click="copy" />
  </InputGroup>
</template>

<script setup lang="ts">
import InputGroup from 'primevue/inputgroup'
import InputGroupAddon from 'primevue/inputgroupaddon'
import InputText from 'primevue/inputtext'
import Button from 'primevue/button'
import { useI18n } from 'vue-i18n'
import { showToast } from '@/services/toast'

// A single-line, labelled, read-only value with a copy button.
const props = defineProps<{ label: string; value: string }>()

const { t } = useI18n()

const copy = () => {
  navigator.clipboard
    .writeText(props.value)
    .then(() => showToast(t('upload.copied'), 'success'))
    .catch(() => showToast(t('upload.copy_failed'), 'error'))
}
</script>

<style scoped>
.label {
  flex: 0 0 6rem;
  font-size: 0.8rem;
}
</style>
