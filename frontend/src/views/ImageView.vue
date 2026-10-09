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
      <dd>{{ humanSize(meta.size) }}</dd>
      <dt>{{ $t('image.format') }}</dt>
      <dd>{{ meta.mime }}</dd>
    </dl>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRoute } from 'vue-router'
import http from '@/services/api/http'

interface Metadata {
  id: string
  ext: string
  mime: string
  original_name: string | null
  size: number
  width: number
  height: number
  created_at: string
  url: string
}

const route = useRoute()
const meta = ref<Metadata | null>(null)

http.get<Metadata>(`/api/i/${route.params.id}`).then((response) => {
  meta.value = response.data
})

const humanSize = (bytes: number) => {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(1)} ${units[unit]}`
}
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
