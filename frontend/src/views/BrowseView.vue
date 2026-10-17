<template>
  <div class="browse">
    <div class="toolbar">
      <Select
        :modelValue="sortBy"
        size="small"
        :options="Object.values(ImagesSortBy).map((value) => ({ label: $t(`browse.sort.${value}`), value }))"
        option-label="label"
        option-value="value"
        @update:modelValue="(value: ImagesSortBy) => setSort(value, order)"
      />
      <Button
        :icon="order === SortOrder.Asc ? 'pi pi-sort-amount-up-alt' : 'pi pi-sort-amount-down'"
        size="small"
        severity="secondary"
        :title="$t(`browse.order.${order}`)"
        :aria-label="$t(`browse.order.${order}`)"
        @click="setSort(sortBy, order === SortOrder.Asc ? SortOrder.Desc : SortOrder.Asc)"
      />
    </div>
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
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import Paginator, { type PageState } from 'primevue/paginator'
import Select from 'primevue/select'
import Button from 'primevue/button'
import { api } from '@/services/api/http'
import { showToast } from '@/services/toast'
import { ImagesSortBy, SortOrder, type ImageSummary } from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

const images = ref<ImageSummary[]>([])
const page = ref(1)
const perPage = ref(100)
const total = ref(0)

const sortBy = computed(() => (route.query.sort_by as ImagesSortBy | undefined) ?? ImagesSortBy.CreatedAt)
const order = computed(() => (route.query.order as SortOrder | undefined) ?? SortOrder.Desc)

const load = (target: number) => {
  api
    .images(target, sortBy.value, order.value)
    .then((response) => {
      images.value = response.data.images
      page.value = response.data.page
      perPage.value = response.data.per_page
      total.value = response.data.total
    })
    .catch(() => {
      showToast(t('browse.load_error'), 'error')
    })
}

// A new order starts back at the first page.
const setSort = (sort_by: ImagesSortBy, order: SortOrder) => router.push({ query: { sort_by, order } })

const onPage = (event: PageState) => router.push({ query: { ...route.query, page: event.page + 1 } })

watch(
  () => route.query,
  (query) => load(Number(query.page) || 1),
  { immediate: true },
)
</script>

<style scoped>
.browse {
  padding: 24px 16px;
}
.toolbar {
  display: flex;
  justify-content: center;
  gap: 8px;
  margin-bottom: 16px;
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
