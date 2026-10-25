<template>
  <div class="legal-wrap">
    <h2>{{ $t(`legal.${field}`) }}</h2>
    <p v-if="!loading" class="legal-text">{{ text || $t('legal.not_set') }}</p>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { api } from '@/services/api/http'
import type { UnauthElysiaSettings } from '@/services/api-schema'

const { field } = defineProps<{ field: 'terms_of_service' | 'privacy_policy' }>()
const settings = ref<UnauthElysiaSettings>()
const loading = ref(true)
const text = computed(() => settings.value?.[field])

api
  .getElysiaUnauthSettings()
  .then((response) => {
    settings.value = response.data
  })
  .finally(() => {
    loading.value = false
  })
</script>

<style scoped>
.legal-wrap {
  max-width: 720px;
  margin: 0 auto;
  padding: 20px;
}
.legal-text {
  white-space: pre-wrap;
}
</style>
