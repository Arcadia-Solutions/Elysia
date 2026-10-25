<template>
  <div class="legal">
    <Checkbox v-model="accepted" inputId="accept-legal" binary size="small" />
    <label for="accept-legal">
      {{ $t('legal.agree') }}
      <router-link v-if="termsOfService" to="/terms" target="_blank">{{ $t('legal.terms_of_service') }}</router-link>
      <template v-if="termsOfService && privacyPolicy"> & </template>
      <router-link v-if="privacyPolicy" to="/privacy" target="_blank">{{ $t('legal.privacy_policy') }}</router-link>
    </label>
  </div>
</template>

<script setup lang="ts">
import Checkbox from 'primevue/checkbox'

const accepted = defineModel<boolean>({ required: true })
defineProps<{ termsOfService: string; privacyPolicy: string }>()
</script>

<style scoped>
.legal {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  font-size: 0.875rem;
}
.legal a {
  font-weight: bold;
  text-decoration: none;
  color: inherit;
}
</style>
