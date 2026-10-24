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
        <RunProgress :progress />
        <small>{{ $t('fixtures.progress', { done: progress.done, total: progress.total }) }}</small>
      </template>
      <Message v-else-if="progress?.error" severity="error" size="small">{{ progress.error }}</Message>
      <Button :label="$t('fixtures.generate')" size="small" :loading="progress?.running" @click="generate" />
    </div>
  </Dialog>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Dialog from 'primevue/dialog'
import InputNumber from 'primevue/inputnumber'
import InputGroup from 'primevue/inputgroup'
import InputText from 'primevue/inputtext'
import Message from 'primevue/message'
import Button from 'primevue/button'
import RunProgress from '@/components/RunProgress.vue'
import { api } from '@/services/api/http'
import { pollProgress } from '@/services/poll'
import { showToast } from '@/services/toast'
import type { FixturesParams, FixturesProgress } from '@/services/api-schema'

const emit = defineEmits<{ done: [] }>()
const { t } = useI18n()
const visible = defineModel<boolean>('visible', { required: true })
const params = ref<FixturesParams>({ image_count: 300, user_count: 12, history_days: 3 * 365, password: 'password123' })
const showPassword = ref(true)
const progress = ref<FixturesProgress>()

// Keeps polling with the dialog closed, so the bar is current when it reopens.
const refresh = pollProgress(api.getFixturesProgress, progress, (ended) => {
  if (!ended.error) showToast(t('fixtures.done', { count: ended.done }), 'success')
  emit('done')
})

// Mark the run as started so the first poll sees it end, even when it finishes before that poll.
const generate = () =>
  api.generateFixtures(params.value).then(() => {
    progress.value = { running: true, done: 0, total: params.value.image_count }
    refresh()
  })
</script>

<style scoped>
.dialog-fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 8px;
}
</style>
