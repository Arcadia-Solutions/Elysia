<template>
  <nav class="pagination" :aria-label="$t('pagination.label')">
    <Button v-bind="navigation" icon="pi pi-angle-double-left" :disabled="page === 1" :aria-label="$t('pagination.first')" @click="emit('goToPage', 1)" />
    <Button v-bind="navigation" icon="pi pi-angle-left" :disabled="page === 1" :aria-label="$t('pagination.prev')" @click="emit('goToPage', page - 1)" />
    <Button
      v-for="range in ranges"
      :key="range.page"
      :label="range.label"
      size="small"
      rounded
      :text="range.page !== page"
      :severity="range.page === page ? undefined : 'secondary'"
      :aria-current="range.page === page ? 'page' : undefined"
      @click="range.page !== page && emit('goToPage', range.page)"
    />
    <Button
      v-bind="navigation"
      icon="pi pi-angle-right"
      :disabled="page === totalPages"
      :aria-label="$t('pagination.next')"
      @click="emit('goToPage', page + 1)"
    />
    <Button
      v-bind="navigation"
      icon="pi pi-angle-double-right"
      :disabled="page === totalPages"
      :aria-label="$t('pagination.last')"
      @click="emit('goToPage', totalPages)"
    />
  </nav>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import Button from 'primevue/button'

const props = defineProps<{
  page: number
  pageSize: number
  totalItems: number
}>()

const emit = defineEmits<{
  goToPage: [number]
}>()

const navigation = { size: 'small', text: true, rounded: true, severity: 'secondary' } as const

const totalPages = computed(() => Math.ceil(props.totalItems / props.pageSize))

// Up to 15 pages around the current one, each labelled with the items it holds.
const ranges = computed(() => {
  const maxVisible = 15
  const end = Math.min(totalPages.value, Math.max(1, props.page - Math.floor(maxVisible / 2)) + maxVisible - 1)
  const start = Math.max(1, end - maxVisible + 1)
  return Array.from({ length: end - start + 1 }, (_, i) => {
    const page = start + i
    return { page, label: `${(page - 1) * props.pageSize + 1}-${Math.min(page * props.pageSize, props.totalItems)}` }
  })
})
</script>

<style scoped>
.pagination {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  align-items: center;
  gap: 2px;
  margin: 12px 0;
  font-variant-numeric: tabular-nums;
}
</style>
