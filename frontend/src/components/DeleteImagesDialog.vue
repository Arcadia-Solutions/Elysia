<template>
  <Dialog v-model:visible="visible" modal :header="$t('trash.delete', count)" :style="{ width: '25rem' }" @show="permanent = false">
    <div class="dialog-fields">
      <p>{{ $t(permanent ? 'trash.delete_permanent_confirm' : 'trash.delete_confirm', { count }, count) }}</p>
      <div class="skip-trash">
        <Checkbox v-model="permanent" inputId="skip-trash" binary size="small" />
        <label for="skip-trash">{{ $t('trash.skip_trash') }}</label>
      </div>
      <Button :label="$t('trash.delete', count)" icon="pi pi-trash" severity="danger" size="small" :loading @click="remove" />
    </div>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Dialog from 'primevue/dialog'
import Checkbox from 'primevue/checkbox'
import Button from 'primevue/button'
import { api } from '@/services/api/http'
import { showToast } from '@/services/toast'

const props = defineProps<{ ids: string[] }>()
const emit = defineEmits<{ deleted: [] }>()
const visible = defineModel<boolean>('visible', { required: true })
const { t } = useI18n()
const permanent = ref(false)
const loading = ref(false)
const count = computed(() => props.ids.length)

const remove = () => {
  loading.value = true
  api
    .trashImages({ ids: props.ids, permanent: permanent.value })
    .then(() => {
      showToast(t(permanent.value ? 'trash.deleted_permanent' : 'trash.deleted', { count: count.value }, count.value), 'success')
      visible.value = false
      emit('deleted')
    })
    .finally(() => {
      loading.value = false
    })
}
</script>

<style scoped>
.dialog-fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 8px;
}
.skip-trash {
  display: flex;
  align-items: center;
  gap: 8px;
}
</style>
