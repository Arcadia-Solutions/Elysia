<template>
  <div v-if="meta" class="image-wrap">
    <img :src="meta.url" :alt="meta.original_name ?? meta.id" class="image" />
    <dl class="meta">
      <template v-if="meta.original_name">
        <dt>{{ $t('image.name') }}</dt>
        <dd>{{ meta.original_name }}</dd>
      </template>
      <dt>{{ $t('image.uploaded_at') }}</dt>
      <dd>{{ new Date(meta.created_at).toLocaleString('sv-SE') }}</dd>
      <dt>{{ $t('image.dimensions') }}</dt>
      <dd>{{ meta.width }} × {{ meta.height }}</dd>
      <dt>{{ $t('image.size') }}</dt>
      <dd>{{ formatBytes(meta.size) }}</dd>
      <dt>{{ $t('image.format') }}</dt>
      <dd>{{ meta.mime }}</dd>
    </dl>
  </div>
</template>

<script setup lang="ts">
import { formatBytes } from '@/services/format'
import { ref } from 'vue'
import { useRoute } from 'vue-router'
import { api } from '@/services/api/http'
import type { MetadataResponse } from '@/services/api-schema'

const route = useRoute()
const meta = ref<MetadataResponse | null>(null)

api.metadata(String(route.params.id)).then((response) => {
  meta.value = response.data
})
</script>

<style scoped>
.image-wrap {
  min-height: 85vh;
  padding: 24px 16px;
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  align-items: center;
  gap: 24px;
}
.image {
  width: 70vw;
  max-height: 85vh;
  object-fit: contain;
  border-radius: 6px;
}
.meta {
  flex: 0 1 240px;
  margin: 0;
  display: grid;
  grid-template-columns: auto;
  gap: 2px 0;
}
.meta dt {
  color: var(--p-text-muted-color);
  font-size: 0.875rem;
}
.meta dd {
  margin: 0 0 12px;
  word-break: break-word;
}
</style>
