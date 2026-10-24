<template>
  <ProgressBar :value="progress.total ? Math.round((progress.done / progress.total) * 100) : 0" :showValue="false" style="height: 6px" />
</template>

<script setup lang="ts">
import ProgressBar from 'primevue/progressbar'

defineProps<{ progress: { done: number; total: number } }>()
</script>
