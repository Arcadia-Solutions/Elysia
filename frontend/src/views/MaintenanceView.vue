<template>
  <div class="maintenance">
    <!-- Lazy: only the shown tab is mounted, so only it loads and polls. Switching tabs drops the other tab's query. -->
    <Tabs :value="tab" lazy @update:value="(value) => router.push({ query: { tab: value === TABS[0] ? undefined : value } })">
      <TabList>
        <Tab value="unreferenced_images">{{ $t('maintenance.unreferenced_images.title') }}</Tab>
        <Tab value="similar_images">{{ $t('maintenance.similar_images.title') }}</Tab>
        <Tab value="chevereto_import">{{ $t('maintenance.chevereto_import.title') }}</Tab>
      </TabList>
      <TabPanels>
        <TabPanel value="unreferenced_images">
          <UnreferencedImages />
        </TabPanel>
        <TabPanel value="similar_images">
          <SimilarImages />
        </TabPanel>
        <TabPanel value="chevereto_import">
          <CheveretoImport />
        </TabPanel>
      </TabPanels>
    </Tabs>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Tabs from 'primevue/tabs'
import TabList from 'primevue/tablist'
import Tab from 'primevue/tab'
import TabPanels from 'primevue/tabpanels'
import TabPanel from 'primevue/tabpanel'
import CheveretoImport from '@/components/maintenance/CheveretoImport.vue'
import SimilarImages from '@/components/maintenance/SimilarImages.vue'
import UnreferencedImages from '@/components/maintenance/UnreferencedImages.vue'

const TABS = ['unreferenced_images', 'similar_images', 'chevereto_import']

const route = useRoute()
const router = useRouter()
const tab = computed(() => TABS.find((value) => value === route.query.tab) ?? TABS[0])
</script>

<style scoped>
.maintenance {
  padding: 24px 16px;
}
</style>
