<template>
  <div class="controls">
    <Select
      :modelValue="sortBy"
      size="small"
      :options="sortOptions"
      option-label="label"
      option-value="value"
      @update:modelValue="(value: SortBy) => emit('sort', value, order)"
    />
    <Button
      :icon="order === SortOrder.Asc ? 'pi pi-sort-amount-up-alt' : 'pi pi-sort-amount-down'"
      size="small"
      severity="secondary"
      :title="$t(`browse.order.${order}`)"
      :aria-label="$t(`browse.order.${order}`)"
      @click="emit('sort', sortBy, order === SortOrder.Asc ? SortOrder.Desc : SortOrder.Asc)"
    />
    <PageSizeSelect :modelValue="pageSize" @update:modelValue="(size: number) => emit('changePageSize', size)" />
    <SelectButton
      v-model="display"
      size="small"
      :allowEmpty="false"
      :options="[
        { icon: 'pi pi-th-large', label: $t('browse.display.grid'), value: 'grid' },
        { icon: 'pi pi-list', label: $t('browse.display.table'), value: 'table' },
      ]"
      option-value="value"
      data-key="value"
    >
      <template #option="{ option }">
        <i :class="option.icon" :title="option.label" :aria-label="option.label" />
      </template>
    </SelectButton>
  </div>
</template>

<script lang="ts">
import { ref, watch } from 'vue'

// Remembered per browser, shared by every list.
export const display = ref<'grid' | 'table'>(localStorage.getItem('image_display') === 'table' ? 'table' : 'grid')
watch(display, (value) => localStorage.setItem('image_display', value))
</script>

<script setup lang="ts" generic="SortBy extends string">
import Select from 'primevue/select'
import SelectButton from 'primevue/selectbutton'
import Button from 'primevue/button'
import PageSizeSelect from '@/components/PageSizeSelect.vue'
import { SortOrder } from '@/services/api-schema'

defineProps<{
  sortBy: SortBy
  order: SortOrder
  pageSize: number
  sortOptions: { label: string; value: SortBy }[]
}>()

const emit = defineEmits<{
  changePageSize: [number]
  sort: [SortBy, SortOrder]
}>()
</script>

<style scoped>
.controls {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
  margin-bottom: 30px;
}
</style>
