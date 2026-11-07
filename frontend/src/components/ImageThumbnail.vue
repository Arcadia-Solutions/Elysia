<template>
  <span class="image-thumbnail">
    <Skeleton v-if="!loaded" width="100%" height="100%" class="skeleton" />
    <img :src :alt :class="{ loading: !loaded }" loading="lazy" @load="loaded = true" @error="loaded = true" />
  </span>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import Skeleton from 'primevue/skeleton'

defineProps<{ src: string; alt: string }>()

const loaded = ref(false)
</script>

<style scoped>
.image-thumbnail {
  position: relative;
  display: block;
  overflow: hidden;
}
.skeleton {
  position: absolute;
  inset: 0;
}
img {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: inherit;
}
/* Hidden rather than removed so the browser still fetches it. */
img.loading {
  opacity: 0;
}
</style>
