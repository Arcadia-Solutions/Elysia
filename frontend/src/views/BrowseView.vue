<template>
  <div class="browse">
    <Tabs
      :value="tab"
      class="tabs"
      @update:value="(value) => router.push({ query: { tab: value === 'albums' ? 'albums' : undefined, username: route.query.username } })"
    >
      <TabList>
        <Tab value="images"><i class="pi pi-image" /> {{ $t('browse.tabs.images') }}</Tab>
        <Tab value="albums"><i class="pi pi-images" /> {{ $t('browse.tabs.albums') }}</Tab>
      </TabList>
    </Tabs>
    <div class="toolbar">
      <!-- The trash is admin-only; other users browse their own uploads. -->
      <SelectButton
        v-if="isAdmin() && tab === 'images'"
        :modelValue="status"
        size="small"
        :allowEmpty="false"
        :options="Object.values(ImagesStatus).map((value) => ({ label: $t(`browse.status.${value}`), value }))"
        option-label="label"
        option-value="value"
        @update:modelValue="(value: ImagesStatus) => router.push({ query: { ...route.query, status: value, page: undefined } })"
      />
      <!-- Admins may narrow the list to one user's uploads (exact username), suggested as they type. -->
      <AutoComplete
        v-if="isAdmin()"
        v-model="usernameInput"
        :suggestions="usernameSuggestions"
        :loading="usernamesLoading"
        size="small"
        :placeholder="$t('browse.username_placeholder')"
        @complete="(event: AutoCompleteCompleteEvent) => suggestUsernames(event.query)"
        @option-select="(event: AutoCompleteOptionSelectEvent) => setUsername(event.value)"
        @keydown.enter="setUsername(usernameInput)"
        @clear="setUsername('')"
      />
      <template v-if="selected.size > 0 && tab === 'images'">
        <Button
          v-if="status === ImagesStatus.Trashed"
          :label="$t('trash.restore', { count: selected.size }, selected.size)"
          icon="pi pi-replay"
          size="small"
          @click="restoreSelected"
        />
        <Button
          v-else
          :label="$t('trash.delete', { count: selected.size }, selected.size)"
          icon="pi pi-trash"
          size="small"
          severity="danger"
          @click="trashSelected"
        />
        <Button :label="$t('browse.clear_selection')" icon="pi pi-times" size="small" severity="secondary" @click="selected.clear()" />
      </template>
      <Button
        v-if="tab === 'images' && status === ImagesStatus.Trashed && total > 0"
        :label="$t('browse.empty_trash')"
        icon="pi pi-trash"
        size="small"
        severity="danger"
        @click="emptyTrash"
      />
      <Button v-if="tab === 'albums'" :label="$t('albums.new')" icon="pi pi-plus" size="small" @click="albumDialogVisible = true" />
      <Button
        v-if="isAdmin() && tab === 'images'"
        :label="$t('browse.fixtures')"
        icon="pi pi-database"
        size="small"
        severity="secondary"
        @click="fixturesVisible = true"
      />
    </div>
    <FixturesDialog v-if="isAdmin()" v-model:visible="fixturesVisible" @done="load(1)" />
    <AlbumDialog v-model:visible="albumDialogVisible" @saved="(album) => router.push({ name: 'Album', params: { id: album.id } })" />
    <p v-if="total === 0" class="empty">{{ shownTab === 'albums' ? $t('albums.empty') : $t(`browse.empty.${status}`) }}</p>
    <AlbumResults
      v-else-if="shownTab === 'albums'"
      :albums
      :page
      :pageSize
      :totalItems="total"
      :sortBy="albumsSortBy"
      :order
      @changePage="(page: number) => router.push({ query: { ...route.query, page } })"
      @changePageSize="(size: number) => router.push({ query: { ...route.query, page: undefined, page_size: size } })"
      @sort="setSort"
    />
    <ImageResults
      v-else
      v-model:selected="selected"
      :images
      :page
      :pageSize
      :totalItems="total"
      :sortBy
      :order
      @changePage="(page: number) => router.push({ query: { ...route.query, page } })"
      @changePageSize="(size: number) => router.push({ query: { ...route.query, page: undefined, page_size: size } })"
      @sort="setSort"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import SelectButton from 'primevue/selectbutton'
import Button from 'primevue/button'
import AutoComplete, { type AutoCompleteCompleteEvent, type AutoCompleteOptionSelectEvent } from 'primevue/autocomplete'
import Tabs from 'primevue/tabs'
import TabList from 'primevue/tablist'
import Tab from 'primevue/tab'
import AlbumDialog from '@/components/AlbumDialog.vue'
import AlbumResults from '@/components/AlbumResults.vue'
import FixturesDialog from '@/components/FixturesDialog.vue'
import ImageResults from '@/components/ImageResults.vue'
import { api } from '@/services/api/http'
import { isAdmin } from '@/services/session'
import { showToast } from '@/services/toast'
import {
  AlbumsSortBy,
  ImagesSortBy,
  ImagesStatus,
  SortOrder,
  UsersSortBy,
  type PaginatedResultsAlbumSummaryResultsInner,
  type PaginatedResultsImageSummaryResultsInner,
} from '@/services/api-schema'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

const images = ref<PaginatedResultsImageSummaryResultsInner[]>([])
const page = ref(1)
const total = ref(0)
const albums = ref<PaginatedResultsAlbumSummaryResultsInner[]>([])
const fixturesVisible = ref(false)
const albumDialogVisible = ref(false)
// Selection survives page changes, so images from several pages can go at once.
const selected = ref(new Set<string>())

const tab = computed(() => (route.query.tab === 'albums' ? 'albums' : 'images'))
// The tab the shown results belong to; lags `tab` until its first page arrives.
const shownTab = ref(tab.value)
// Only the latest request's response is shown, so a slow one can't overwrite a newer one.
let latestRequest = 0
const sortBy = computed(() => (route.query.sort_by as ImagesSortBy | undefined) ?? ImagesSortBy.CreatedAt)
const albumsSortBy = computed(() => (route.query.sort_by as AlbumsSortBy | undefined) ?? AlbumsSortBy.CreatedAt)
const order = computed(() => (route.query.order as SortOrder | undefined) ?? SortOrder.Desc)
const status = computed(() => (route.query.status as ImagesStatus | undefined) ?? ImagesStatus.Available)
const username = computed(() => route.query.username as string | undefined)
const pageSize = computed(() => Number(route.query.page_size) || 100)
const usernameInput = ref(username.value ?? '')
const usernameSuggestions = ref<string[]>([])
const usernamesLoading = ref(false)
watch(username, (value) => (usernameInput.value = value ?? ''))

const suggestUsernames = (search: string) => {
  usernamesLoading.value = true
  api
    .listUsers(1, 10, UsersSortBy.Username, SortOrder.Asc, search.trim())
    .then((response) => {
      usernameSuggestions.value = response.data.results.map((user) => user.username)
    })
    .finally(() => {
      usernamesLoading.value = false
    })
}

const setUsername = (value: string) => router.push({ query: { ...route.query, username: value.trim() || undefined, page: undefined } })

const load = (target: number) => {
  const request = ++latestRequest
  const loadingTab = tab.value
  if (loadingTab === 'albums') {
    api.listAlbums(target, pageSize.value, albumsSortBy.value, order.value, username.value).then((response) => {
      if (request !== latestRequest) return
      shownTab.value = loadingTab
      albums.value = response.data.results
      page.value = response.data.page
      total.value = response.data.total_items
    })
    return
  }
  api.images(target, pageSize.value, sortBy.value, order.value, status.value, username.value).then((response) => {
    if (request !== latestRequest) return
    shownTab.value = loadingTab
    images.value = response.data.results
    page.value = response.data.page
    total.value = response.data.total_items
  })
}

// A new order starts back at the first page.
const setSort = (sort_by: ImagesSortBy | AlbumsSortBy, order: SortOrder) =>
  router.push({ query: { tab: route.query.tab, status: route.query.status, username: route.query.username, page_size: route.query.page_size, sort_by, order } })

const trashSelected = () => {
  const count = selected.value.size
  if (!confirm(t('trash.delete_confirm', { count }, count))) return
  api.trashImages({ ids: [...selected.value] }).then(() => {
    showToast(t('trash.deleted', { count }, count), 'success')
    selected.value.clear()
    load(page.value)
  })
}

const restoreSelected = () => {
  const count = selected.value.size
  api.restoreImages({ ids: [...selected.value] }).then(() => {
    showToast(t('trash.restored', { count }, count), 'success')
    selected.value.clear()
    load(page.value)
  })
}

const emptyTrash = () => {
  if (!confirm(t('browse.empty_trash_confirm', { count: total.value }, total.value))) return
  api.emptyTrash().then(() => {
    showToast(t('browse.trash_emptied'), 'success')
    load(1)
  })
}

watch(
  () => route.query,
  (query) => load(Number(query.page) || 1),
  { immediate: true },
)
watch([status, tab], () => selected.value.clear())
</script>

<style scoped>
.browse {
  padding: 24px 16px;
}
.tabs {
  margin-bottom: 16px;
}
.tabs :deep(.p-tablist-tab-list) {
  justify-content: center;
}
.tabs :deep(.p-tab) {
  display: flex;
  align-items: center;
  gap: 8px;
}
.toolbar {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
  margin-bottom: 16px;
}
.empty {
  text-align: center;
  color: var(--p-text-muted-color);
}
</style>
