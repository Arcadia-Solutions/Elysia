<template>
  <Select
    v-model="pageSize"
    :options="PAGE_SIZES.map((size) => ({ label: $t('pagination.per_page', { count: size }), value: size }))"
    option-label="label"
    option-value="value"
    size="small"
    :aria-label="$t('pagination.page_size')"
  />
</template>

<script setup lang="ts">
import Select from 'primevue/select'

const PAGE_SIZES = [25, 50, 100, 200, 500]

const pageSize = defineModel<number>({ required: true })
</script>
