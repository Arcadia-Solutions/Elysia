<template>
  <div class="reports">
    <p v-if="page && page.total_items === 0" class="empty">{{ $t('reports.empty') }}</p>
    <template v-else-if="page">
      <PageSizeSelect
        :modelValue="pageSize"
        class="page-size"
        @update:modelValue="(size: number) => router.push({ query: { ...route.query, page: undefined, page_size: size } })"
      />
      <PaginatedResults
        :page="page.page"
        :pageSize
        :totalItems="page.total_items"
        @changePage="(number: number) => router.push({ query: { ...route.query, page: number } })"
      >
        <div v-for="reported in page.results" :key="reported.image.id" class="reported">
          <router-link :to="{ name: 'Image', params: { id: reported.image.id } }" class="thumb">
            <img :src="reported.image.thumbnail_url" :alt="reported.image.id" loading="lazy" />
          </router-link>
          <div class="details">
            <small>
              {{ $t('reports.uploaded_by', { username: reported.image.uploader_username }) }},
              <span :title="formatDate(reported.image.created_at)">{{ timeAgo(reported.image.created_at) }}</span>
            </small>
            <ul class="list">
              <li v-for="(report, index) in reported.reports" :key="index">
                <strong>{{ report.reporter_username }}</strong>
                (<span :title="formatDate(report.created_at)">{{ timeAgo(report.created_at) }}</span
                >): {{ report.reason }}
              </li>
            </ul>
            <div class="actions">
              <Button :label="$t('trash.delete', 1)" icon="pi pi-trash" severity="danger" size="small" @click="deleteId = reported.image.id" />
              <Button :label="$t('reports.dismiss')" icon="pi pi-check" severity="secondary" size="small" @click="dismiss(reported.image.id)" />
            </div>
          </div>
        </div>
      </PaginatedResults>
    </template>
    <DeleteImagesDialog
      :visible="deleteId !== null"
      :ids="deleteId ? [deleteId] : []"
      @update:visible="(visible: boolean) => visible || (deleteId = null)"
      @deleted="load(currentPage)"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import Button from 'primevue/button'
import DeleteImagesDialog from '@/components/DeleteImagesDialog.vue'
import PageSizeSelect from '@/components/PageSizeSelect.vue'
import PaginatedResults from '@/components/PaginatedResults.vue'
import { api } from '@/services/api/http'
import { formatDate, timeAgo } from '@/services/format'
import { showToast } from '@/services/toast'
import type { PaginatedResultsReportedImage } from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const currentPage = computed(() => Number(route.query.page) || 1)
// Reported images per page.
const pageSize = computed(() => Number(route.query.page_size) || 25)
const page = ref<PaginatedResultsReportedImage>()
const deleteId = ref<string | null>(null)

const load = (number: number) =>
  api.listReports(number, pageSize.value).then((response): unknown => {
    // Deleting or dismissing can empty the last page: go to the new last one.
    if (response.data.results.length === 0 && number > 1)
      return router.replace({ query: { ...route.query, page: Math.max(1, Math.ceil(response.data.total_items / pageSize.value)) } })
    page.value = response.data
  })

const dismiss = (id: string) =>
  api.dismissReports({ ids: [id] }).then(() => {
    showToast(t('reports.dismissed'), 'success')
    load(currentPage.value)
  })

watch(
  () => route.query,
  () => load(currentPage.value),
  { immediate: true },
)
</script>

<style scoped>
.reports {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 24px 16px;
  max-width: 900px;
  margin: 0 auto;
}
.page-size {
  align-self: flex-end;
}
.empty,
.details small {
  margin: 0;
  color: var(--p-text-muted-color);
}
.reported {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  padding: 12px;
  border: 1px solid var(--p-content-border-color);
  border-radius: 8px;
}
.thumb {
  flex: 0 0 160px;
  height: 120px;
  border-radius: 4px;
  overflow: hidden;
  background: var(--p-content-border-color);
}
.thumb img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.details {
  flex: 1 1 300px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.list {
  margin: 0;
  padding-left: 18px;
  word-break: break-word;
}
.actions {
  display: flex;
  gap: 8px;
}
</style>
