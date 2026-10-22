<template>
  <div class="maintenance">
    <Tabs value="unreferenced_images">
      <TabList>
        <Tab value="unreferenced_images">{{ $t('maintenance.unreferenced_images.title') }}</Tab>
      </TabList>
      <TabPanels>
        <TabPanel value="unreferenced_images">
          <UnreferencedImages />
        </TabPanel>
      </TabPanels>
    </Tabs>
  </div>
</template>

<script setup lang="ts">
import Tabs from 'primevue/tabs'
import TabList from 'primevue/tablist'
import Tab from 'primevue/tab'
import TabPanels from 'primevue/tabpanels'
import TabPanel from 'primevue/tabpanel'
import UnreferencedImages from '@/components/maintenance/UnreferencedImages.vue'
</script>

<style scoped>
.maintenance {
  padding: 24px 16px;
}
</style>
