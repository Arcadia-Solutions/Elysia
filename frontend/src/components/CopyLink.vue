<template>
  <InputGroup>
    <InputGroupAddon class="label">{{ label }}</InputGroupAddon>
    <Textarea v-if="value.includes('\n')" :value="value" size="small" :rows="Math.min(value.split('\n').length, 8)" readonly fluid class="lines" />
    <InputText v-else :value="value" size="small" readonly fluid />
    <Button icon="pi pi-copy" size="small" :title="$t('upload.copy')" :aria-label="$t('upload.copy')" @click="copy" />
  </InputGroup>
</template>

<script setup lang="ts">
import InputGroup from 'primevue/inputgroup'
import InputGroupAddon from 'primevue/inputgroupaddon'
import InputText from 'primevue/inputtext'
import Textarea from 'primevue/textarea'
import Button from 'primevue/button'
import { useI18n } from 'vue-i18n'
import { showToast } from '@/services/toast'

// A labelled, read-only value with a copy button. Multi-line values get a textarea.
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
.lines {
  white-space: pre;
  resize: vertical;
}
</style>
