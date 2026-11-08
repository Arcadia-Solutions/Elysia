<template>
  <section class="tool">
    <h3>{{ $t('maintenance.data_tools.recalculate_sizes.title') }}</h3>
    <small>{{ $t('maintenance.data_tools.recalculate_sizes.help') }}</small>
    <div>
      <Button
        :label="$t('maintenance.data_tools.recalculate_sizes.title')"
        icon="pi pi-calculator"
        size="small"
        outlined
        :loading="loading"
        @click="recalculate"
      />
    </div>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Button from 'primevue/button'
import { api } from '@/services/api/http'
import { showToast } from '@/services/toast'

const { t } = useI18n()
const loading = ref(false)

const recalculate = () => {
  if (!confirm(t('maintenance.data_tools.recalculate_sizes.confirm'))) return
  loading.value = true
  api
    .recalculateSizes()
    .then(({ data }) => showToast(t('maintenance.data_tools.recalculate_sizes.done', { ...data }), 'success'))
    .finally(() => (loading.value = false))
}
</script>
