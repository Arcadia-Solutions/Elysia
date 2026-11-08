<template>
  <span v-for="(kind, index) in KINDS" :key="kind" class="count">
    {{ $t(`image.view_counts.${kind}`, { count: counts[FIELDS[kind]] }) }}
    <Button
      v-if="isAdmin()"
      v-tooltip.top="resetLabel(kind)"
      icon="pi pi-refresh"
      severity="secondary"
      size="small"
      text
      rounded
      class="reset"
      :aria-label="resetLabel(kind)"
      @click="reset(kind)"
    /><template v-if="index < KINDS.length - 1">,</template>
  </span>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import Button from 'primevue/button'
import Tooltip from 'primevue/tooltip'
import { api } from '@/services/api/http'
import { isAdmin } from '@/services/session'
import { showToast } from '@/services/toast'
import { ViewKind, type ReferrerViews } from '@/services/api-schema'

const vTooltip = Tooltip
const FIELDS = { image: 'views', medium: 'medium_views', thumbnail: 'thumbnail_views' } as const
const KINDS = [ViewKind.Image, ViewKind.Medium, ViewKind.Thumbnail] as const

// Without a referrer, the counts are the image's totals and a reset covers every referrer.
const props = defineProps<{ id: string; counts: Omit<ReferrerViews, 'referrer'>; referrer?: string }>()
const emit = defineEmits<{ reset: [] }>()
const { t } = useI18n()

const resetLabel = (kind: ViewKind) =>
  t('image.reset_views.tooltip', {
    kind: t(`image.view_kinds.${kind}`),
    referrer: props.referrer === undefined ? t('image.reset_views.all_referrers') : props.referrer || t('image.no_referrer'),
  })

const reset = (kind: ViewKind) => {
  if (!confirm(t('image.reset_views.confirm', { action: resetLabel(kind) }))) return
  api.resetViews({ id: props.id, kind, referrers: props.referrer === undefined ? [] : [props.referrer] }).then(() => {
    showToast(t('image.reset_views.done'), 'success')
    emit('reset')
  })
}
</script>

<style scoped>
.count {
  white-space: nowrap;
}
.count + .count {
  margin-left: 4px;
}
.reset {
  width: 1.5rem;
  height: 1.5rem;
  padding: 0;
  vertical-align: middle;
}
</style>
