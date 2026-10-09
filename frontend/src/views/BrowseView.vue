<template>
  <div class="browse">
    <p v-if="total === 0" class="empty">{{ $t('browse.empty') }}</p>
    <template v-else>
      <div class="grid">
        <router-link v-for="image in images" :key="image.id" :to="{ name: 'Image', params: { id: image.id } }" class="cell">
          <img :src="image.thumbnail_url" :alt="image.id" loading="lazy" />
        </router-link>
      </div>
      <Paginator :rows="perPage" :totalRecords="total" :first="(page - 1) * perPage" @page="onPage" />
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Paginator, { type PageState } from 'primevue/paginator'
import http from '@/services/api/http'
import { showToast } from '@/services/toast'
import type { ImageSummary, ImagesPage } from '@/services/api-schema'

const { t } = useI18n()

const images = ref<ImageSummary[]>([])
const page = ref(1)
const perPage = ref(100)
const total = ref(0)

const load = (target: number) => {
  http
    .get<ImagesPage>(`/api/images?page=${target}`)
    .then((response) => {
      images.value = response.data.images
      page.value = response.data.page
      perPage.value = response.data.per_page
      total.value = response.data.total
    })
    .catch(() => {
      showToast('', t('browse.load_error'), 'error')
    })
}

const onPage = (event: PageState) => load(event.page + 1)

load(1)
</script>

<style scoped>
.browse {
  padding: 24px 16px;
}
.empty {
  text-align: center;
  color: var(--p-text-muted-color);
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 8px;
}
.cell {
  aspect-ratio: 1;
  overflow: hidden;
  border-radius: 6px;
  background: var(--p-content-border-color, #eee);
}
.cell img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
</style>
