<template>
  <section class="tool">
    <h3>{{ $t('maintenance.data_tools.compute_checksums.title') }}</h3>
    <small>{{ $t('maintenance.data_tools.compute_checksums.help') }}</small>
    <div>
      <Button :label="$t('maintenance.data_tools.compute_checksums.title')" icon="pi pi-hashtag" size="small" outlined @click="visible = true" />
    </div>
    <Dialog v-model:visible="visible" modal :header="$t('maintenance.data_tools.compute_checksums.title')" :style="{ width: '28rem' }" @show="open">
      <div class="content">
        <ul v-if="coverage" class="coverage">
          <li v-for="count in coverage.counts" :key="count.algorithm">
            {{ $t(`checksums.${count.algorithm}`) }}:
            {{ $t('maintenance.data_tools.compute_checksums.coverage', { count: count.files, total: coverage.total_files }) }}
          </li>
        </ul>
        <MultiSelect
          v-model="algorithms"
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
          :placeholder="$t('maintenance.data_tools.compute_checksums.algorithms')"
        />
        <template v-if="progress?.running">
          <RunProgress :progress />
          <small>{{ $t('maintenance.data_tools.compute_checksums.progress', { done: progress.done, total: progress.total }) }}</small>
        </template>
        <Message v-else-if="progress?.error" severity="error" size="small">{{ progress.error }}</Message>
        <div>
          <Button
            :label="$t('maintenance.data_tools.compute_checksums.start')"
            icon="pi pi-play"
            size="small"
            :loading="progress?.running"
            :disabled="algorithms.length === 0"
            @click="start"
          />
        </div>
      </div>
    </Dialog>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Button from 'primevue/button'
import Dialog from 'primevue/dialog'
import Message from 'primevue/message'
import MultiSelect from 'primevue/multiselect'
import RunProgress from '@/components/RunProgress.vue'
import { api } from '@/services/api/http'
import { pollProgress } from '@/services/poll'
import { showToast } from '@/services/toast'
import { ChecksumAlgorithm, type ChecksumCounts, type ChecksumsProgress } from '@/services/api-schema'

const { t } = useI18n()
const visible = ref(false)
const algorithms = ref<ChecksumAlgorithm[]>([])
const coverage = ref<ChecksumCounts>()
const progress = ref<ChecksumsProgress>()

const loadCoverage = () =>
  api.getChecksumCounts().then(({ data }) => {
    coverage.value = data
  })

const refresh = pollProgress(api.getChecksumsProgress, progress, (data) => {
  if (!data.error) showToast(t('maintenance.data_tools.compute_checksums.done', { done: data.done, missing: data.missing }), 'success')
  loadCoverage()
})

const open = () => {
  loadCoverage()
  refresh()
}

const start = () => {
  api.computeChecksums({ algorithms: algorithms.value }).then(() => {
    // Mark the run as started so the first poll sees it end, even when it finishes before that poll.
    progress.value = { running: true, done: 0, total: 0, missing: 0, error: null }
    refresh()
  })
}
</script>

<style scoped>
.content {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.coverage {
  margin: 0;
  padding-left: 18px;
}
</style>
