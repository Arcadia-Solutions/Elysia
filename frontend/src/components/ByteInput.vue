<template>
  <div class="size-row">
    <InputNumber v-model="state.value" size="small" :min="0" fluid :disabled :invalid />
    <Select
      v-model="state.unit"
      size="small"
      :disabled
      :options="[
        { label: 'B', value: 1 },
        { label: 'KB', value: 1024 },
        { label: 'MB', value: 1048576 },
        { label: 'GB', value: 1073741824 },
      ]"
      option-label="label"
      option-value="value"
    />
  </div>
</template>

<script setup lang="ts">
import { reactive, watch } from 'vue'
import InputNumber from 'primevue/inputnumber'
import Select from 'primevue/select'

// A byte count edited as value + unit.
const bytes = defineModel<number>({ required: true })
defineProps<{ disabled?: boolean; invalid?: boolean }>()

const state = reactive({ value: 0, unit: 1048576 })

// Shows an outside count as the largest unit that divides it evenly (10485760
// as "10 MB"), keeping the current unit when the count is 0.
watch(
  bytes,
  (count) => {
    if (count === Math.round(state.value * state.unit)) return
    for (const unit of [1073741824, 1048576, 1024]) {
      if (count >= unit && count % unit === 0) {
        state.value = count / unit
        state.unit = unit
        return
      }
    }
    state.value = count
    if (count !== 0) state.unit = 1
  },
  { immediate: true },
)
watch(state, () => (bytes.value = Math.round(state.value * state.unit)))
</script>

<style scoped>
.size-row {
  display: flex;
  gap: 8px;
}
.size-row > :first-child {
  flex: 1;
}
</style>
