<template>
  <section class="tool">
    <h3>{{ $t('maintenance.data_tools.reset_views.title') }}</h3>
    <small>{{ $t('maintenance.data_tools.reset_views.help') }}</small>
    <div>
      <Button :label="$t('maintenance.data_tools.reset_views.title')" icon="pi pi-refresh" severity="danger" size="small" outlined @click="visible = true" />
    </div>
    <Dialog v-model:visible="visible" modal :header="$t('maintenance.data_tools.reset_views.title')" :style="{ width: '25rem' }" @show="referrers = ''">
      <form class="form" @submit.prevent="reset">
        <SelectButton
          v-model="kind"
          :options="Object.values(ViewKind)"
          :optionLabel="(option: ViewKind) => $t(`image.view_kinds.${option}`)"
          :allowEmpty="false"
          size="small"
        />
        <InputText v-model="referrers" size="small" :placeholder="$t('maintenance.data_tools.reset_views.referrers')" />
        <small>{{ $t('maintenance.data_tools.reset_views.referrers_help') }}</small>
        <Button type="submit" :label="$t('maintenance.data_tools.reset_views.submit')" icon="pi pi-refresh" severity="danger" size="small" />
      </form>
    </Dialog>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Button from 'primevue/button'
import Dialog from 'primevue/dialog'
import InputText from 'primevue/inputtext'
import SelectButton from 'primevue/selectbutton'
import { api } from '@/services/api/http'
import { splitList } from '@/services/format'
import { showToast } from '@/services/toast'
import { ViewKind } from '@/services/api-schema'

const { t } = useI18n()
const visible = ref(false)
const kind = ref<ViewKind>(ViewKind.Image)
const referrers = ref('')

const reset = () => {
  if (!confirm(t('maintenance.data_tools.reset_views.confirm'))) return
  api.resetViews({ kind: kind.value, referrers: splitList(referrers.value) }).then(() => {
    visible.value = false
    showToast(t('image.reset_views.done'), 'success')
  })
}
</script>

<style scoped>
.tool,
.form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.form {
  padding-top: 8px;
}
.tool h3 {
  margin: 0;
}
.tool small,
.form small {
  color: var(--p-text-muted-color);
}
</style>
