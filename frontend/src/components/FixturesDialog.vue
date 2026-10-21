<template>
  <Dialog v-model:visible="visible" modal :header="$t('fixtures.title')" :style="{ width: '25rem' }" @show="refresh">
    <div class="dialog-fields">
      <small>{{ $t('fixtures.help') }}</small>
      <label>{{ $t('fixtures.image_count') }}</label>
      <InputNumber v-model="params.image_count" size="small" :min="1" :max="10000" fluid :disabled="progress?.running" />
      <label>{{ $t('fixtures.user_count') }}</label>
      <InputNumber v-model="params.user_count" size="small" :min="1" fluid :disabled="progress?.running" />
      <label>{{ $t('fixtures.history_days') }}</label>
      <InputNumber v-model="params.history_days" size="small" :min="1" :max="3650" fluid :disabled="progress?.running" />
      <label>{{ $t('fixtures.password') }}</label>
      <InputGroup>
        <InputText v-model="params.password" :type="showPassword ? 'text' : 'password'" size="small" fluid :disabled="progress?.running" />
        <Button
          :icon="showPassword ? 'pi pi-eye-slash' : 'pi pi-eye'"
          size="small"
          severity="secondary"
          :title="$t('fixtures.show_password')"
          :aria-label="$t('fixtures.show_password')"
          @click="showPassword = !showPassword"
        />
      </InputGroup>
      <template v-if="progress?.running">
        <ProgressBar :value="Math.round((progress.done / progress.total) * 100)" :showValue="false" style="height: 6px" />
        <small>{{ $t('fixtures.progress', { done: progress.done, total: progress.total }) }}</small>
      </template>
      <Message v-else-if="progress?.error" severity="error" size="small">{{ progress.error }}</Message>
      <Button :label="$t('fixtures.generate')" size="small" :loading="progress?.running" @click="generate" />
    </div>
  </Dialog>
</template>

<script setup lang="ts">
import { onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Dialog from 'primevue/dialog'
import InputNumber from 'primevue/inputnumber'
import InputGroup from 'primevue/inputgroup'
import InputText from 'primevue/inputtext'
import ProgressBar from 'primevue/progressbar'
import Message from 'primevue/message'
import Button from 'primevue/button'
import { api } from '@/services/api/http'
import { showToast } from '@/services/toast'
import type { FixturesParams, FixturesProgress } from '@/services/api-schema'

const emit = defineEmits<{ done: [] }>()
const { t } = useI18n()
const visible = defineModel<boolean>('visible', { required: true })
const params = ref<FixturesParams>({ image_count: 300, user_count: 12, history_days: 3 * 365, password: 'password123' })
const showPassword = ref(true)
const progress = ref<FixturesProgress>()
let timer: ReturnType<typeof setTimeout> | undefined

// Polls once a second while a run goes, so the bar keeps moving even with the dialog closed.
const refresh = () => {
  clearTimeout(timer)
  api.getFixturesProgress().then((response) => {
    const wasRunning = progress.value?.running
    progress.value = response.data
    if (response.data.running) {
      timer = setTimeout(refresh, 1000)
    } else if (wasRunning) {
      if (!response.data.error) showToast(t('fixtures.done', { count: response.data.done }), 'success')
      emit('done')
    }
  })
}

// Mark the run as started so the first poll sees it end, even when it finishes before that poll.
const generate = () =>
  api.generateFixtures(params.value).then(() => {
    progress.value = { running: true, done: 0, total: params.value.image_count }
    refresh()
  })

onUnmounted(() => clearTimeout(timer))
</script>

<style scoped>
.dialog-fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 8px;
}
</style>
