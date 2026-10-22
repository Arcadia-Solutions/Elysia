<template>
  <PaginationSelector v-if="totalItems > pageSize" :page :pageSize :totalItems @goToPage="emit('changePage', $event)" />
  <slot />
  <PaginationSelector v-if="totalItems > pageSize" :page :pageSize :totalItems @goToPage="goToPageAndScroll" />
</template>

<script setup lang="ts">
import PaginationSelector from '@/components/PaginationSelector.vue'

defineProps<{
  page: number
  pageSize: number
  totalItems: number
}>()

const emit = defineEmits<{
  changePage: [number]
}>()

const goToPageAndScroll = (page: number) => {
  emit('changePage', page)
  window.scrollTo({ top: 0, behavior: 'smooth' })
}
</script>
