<template>
  <div class="dashboard">
    <h2>{{ $t('dashboard.title') }}</h2>

    <div class="filters">
      <FloatLabel>
        <Select
          v-model="period"
          :options="[
            { label: $t('dashboard.periods.last_week'), value: StatsPeriod.LastWeek },
            { label: $t('dashboard.periods.last_month'), value: StatsPeriod.LastMonth },
            { label: $t('dashboard.periods.last_year'), value: StatsPeriod.LastYear },
            { label: $t('dashboard.periods.all_time'), value: StatsPeriod.AllTime },
          ]"
          optionLabel="label"
          optionValue="value"
          size="small"
          inputId="statsPeriod"
        />
        <label for="statsPeriod">{{ $t('dashboard.period') }}</label>
      </FloatLabel>
      <FloatLabel>
        <Select
          v-model="interval"
          :options="Object.values(StatsInterval).map((value) => ({ label: $t(`dashboard.intervals.${value}`), value }))"
          optionLabel="label"
          optionValue="value"
          size="small"
          inputId="statsInterval"
        />
        <label for="statsInterval">{{ $t('dashboard.interval') }}</label>
      </FloatLabel>
    </div>

    <div class="tiles">
      <div v-for="tile in tiles" :key="tile.label" class="tile">
        <small>{{ tile.label }}</small>
        <strong>{{ tile.value }}</strong>
      </div>
    </div>

    <p v-if="stats && stats.periods.length === 0" class="empty">{{ $t('browse.empty.available') }}</p>
    <div v-else-if="stats" class="charts">
      <section v-for="(chart, index) in charts" :key="index">
        <h3>{{ chart.title }}</h3>
        <!-- Remounted per interval: the point count changes, which an in-place update draws without animation. -->
        <Chart :key="statsInterval" class="chart" :options="chart.options" />
      </section>
      <section v-if="stats.referrers.length">
        <h3>{{ $t('dashboard.referrers') }}</h3>
        <Chart class="chart" :options="referrersChart" />
      </section>
    </div>

    <div class="tables">
      <section v-if="stats">
        <h3>{{ $t('dashboard.uploaders') }}</h3>
        <DataTable :value="stats.uploaders" scrollable scrollHeight="400px" size="small">
          <Column field="username" :header="$t('dashboard.username')" />
          <Column :header="$t('dashboard.size')">
            <template #body="{ data }">{{ formatBytes(data.bytes) }}</template>
          </Column>
          <Column :header="$t('dashboard.files')">
            <template #body="{ data }">{{ formatCount(data.files) }}</template>
          </Column>
          <Column :header="$t('dashboard.last_upload')">
            <template #body="{ data }">{{ new Date(data.last_upload).toLocaleString('sv-SE', { timeZone: 'UTC' }) }} UTC</template>
          </Column>
        </DataTable>
      </section>

      <section v-if="stats">
        <h3>{{ $t('dashboard.formats') }}</h3>
        <Chart class="chart" :options="formatsChart" />
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import FloatLabel from 'primevue/floatlabel'
import Select from 'primevue/select'
import { Chart } from 'highcharts-vue'
import type Highcharts from 'highcharts'
import { api } from '@/services/api/http'
import { formatBytes } from '@/services/format'
import { StatsInterval, StatsPeriod, type PeriodTotal, type StatsResponse } from '@/services/api-schema'

const { t } = useI18n()

const formatCount = (value: number) => value.toLocaleString()

// Shallow: the stats are replaced whole, never mutated, and the tooltip reads
// them on every hovered point, so skip the deep reactive proxies.
const stats = shallowRef<StatsResponse>()

// Everything uploaded within the selected window.
const tiles = computed(() => {
  const totals = stats.value?.window
  const uploads = totals?.uploads ?? 0
  const bytes = totals?.uploaded_bytes ?? 0
  return [
    { label: t('dashboard.uploaded_size'), value: formatBytes(bytes) },
    { label: t('dashboard.uploads'), value: formatCount(uploads) },
    { label: t('dashboard.average_size'), value: formatBytes(uploads ? Math.round(bytes / uploads) : 0) },
    { label: t('dashboard.new_users'), value: formatCount(totals?.new_users ?? 0) },
    { label: t('dashboard.active_uploaders'), value: formatCount(totals?.active_uploaders ?? 0) },
  ]
})

const period = ref<StatsPeriod>(StatsPeriod.AllTime)
const interval = ref<StatsInterval>(StatsInterval.Month)
// The interval of the shown stats, which lags `interval` until they load.
const statsInterval = ref<StatsInterval>(interval.value)

// Only the latest request is shown, so a slow earlier one cannot overwrite it.
let latestRequest = 0
watch(
  [period, interval],
  ([requestedPeriod, requestedInterval]) => {
    const request = ++latestRequest
    api.statsOverview(requestedPeriod, requestedInterval).then((response) => {
      if (request !== latestRequest) return
      stats.value = response.data
      statsInterval.value = requestedInterval
    })
  },
  { immediate: true },
)

// Chart colors are theme CSS variables, which follow the system dark mode.
const readThemeColors = () => {
  const style = getComputedStyle(document.documentElement)
  return {
    muted: style.getPropertyValue('--p-text-muted-color'),
    primary: style.getPropertyValue('--p-primary-color'),
    secondary: style.getPropertyValue('--p-orange-500'),
  }
}
const themeColors = ref(readThemeColors())
const darkMode = window.matchMedia('(prefers-color-scheme: dark)')
const onThemeChange = () => {
  themeColors.value = readThemeColors()
}
darkMode.addEventListener('change', onThemeChange)
onUnmounted(() => darkMode.removeEventListener('change', onThemeChange))

// Tooltip label of a step, as precise as the interval: 2026-10-15 14:00 for
// hours down to 2026 for years.
const stepLabel = (time: number) => {
  const iso = new Date(time).toISOString().replace('T', ' ')
  const length = { hour: 16, day: 10, week: 10, month: 7, year: 4 }[statsInterval.value]
  return iso.slice(0, length)
}

interface Measure {
  name: string
  value: (step: PeriodTotal) => number
  format: (value: number) => string
  color: string
}

// Area chart of one value per step for each measure, each on its own y axis
// in the measure's color, with a legend when there are several. Hovering a
// series dims the others. `extra` adds a measure of the step to the tooltip
// only.
const chartOptions = (measures: Measure[], extra?: { label: string; value: (step: PeriodTotal) => string }): Highcharts.Options => {
  const single = measures.length === 1
  const muted = { color: themeColors.value.muted }
  return {
    chart: { type: 'area', backgroundColor: 'transparent' },
    title: { text: undefined },
    credits: { enabled: false },
    legend: { enabled: !single, itemStyle: muted },
    xAxis: { type: 'datetime', labels: { style: muted } },
    yAxis: measures.map((measure, index) => ({
      title: { text: undefined },
      opposite: index > 0,
      // Headroom above the later measures, so their lines peak lower than
      // the first one instead of overlapping it.
      maxPadding: index > 0 ? 0.6 : undefined,
      labels: {
        style: { color: single ? themeColors.value.muted : measure.color },
        formatter: (context) => measure.format(Number(context.value)),
      },
    })),
    series: measures.map((measure, index) => ({
      type: 'area',
      name: measure.name,
      yAxis: index,
      data: (stats.value?.periods ?? []).map((step) => [Date.parse(step.period), measure.value(step)]),
      color: measure.color,
      fillOpacity: 0.15,
      marker: { enabled: false },
      // Hover the series nearest the cursor, not the first one at its x.
      findNearestPointBy: 'xy',
      states: { inactive: { opacity: 0.35 } },
    })),
    // No slide between points, so the tooltip keeps up with the cursor.
    tooltip: {
      animation: false,
      formatter() {
        const step = stats.value?.periods[this.index]
        if (!step) return false
        const lines = measures.map((measure) => `${measure.name}: ${measure.format(measure.value(step))}`)
        if (extra) lines.push(`${extra.label}: ${extra.value(step)}`)
        return [`<b>${stepLabel(this.x)}</b>`, ...lines].join('<br/>')
      },
    },
  }
}

const charts = computed(() => [
  {
    title: t('dashboard.size_and_files_over_time'),
    options: chartOptions([
      { name: t('dashboard.size'), value: (step) => step.bytes, format: formatBytes, color: themeColors.value.primary },
      { name: t('dashboard.files'), value: (step) => step.files, format: formatCount, color: themeColors.value.secondary },
    ]),
  },
  {
    title: t('dashboard.uploads_per', { interval: t(`dashboard.intervals.${statsInterval.value}`).toLowerCase() }),
    options: chartOptions([{ name: t('dashboard.uploads'), value: (step) => step.uploads, format: formatCount, color: themeColors.value.primary }], {
      label: t('dashboard.size'),
      value: (step) => formatBytes(step.uploaded_bytes),
    }),
  },
  {
    title: t('dashboard.deletions_per', { interval: t(`dashboard.intervals.${statsInterval.value}`).toLowerCase() }),
    options: chartOptions([{ name: t('dashboard.deletions'), value: (step) => step.deletions, format: formatCount, color: themeColors.value.secondary }], {
      label: t('dashboard.size'),
      value: (step) => formatBytes(step.deleted_bytes),
    }),
  },
])

// Share of the stored size per format, its file count in the tooltip.
const formatsChart = computed(
  (): Highcharts.Options => ({
    chart: { type: 'pie', backgroundColor: 'transparent' },
    title: { text: undefined },
    credits: { enabled: false },
    series: [
      {
        type: 'pie',
        name: t('dashboard.size'),
        data: (stats.value?.formats ?? []).map((format) => ({ name: format.mime, y: format.bytes })),
        dataLabels: { style: { color: themeColors.value.muted, textOutline: 'none' } },
      },
    ],
    tooltip: {
      formatter() {
        const format = stats.value?.formats[this.index]
        if (!format) return false
        return `<b>${format.mime}</b><br/>${t('dashboard.size')}: ${formatBytes(format.bytes)} (${(this.percentage ?? 0).toFixed(1)}%)<br/>${t('dashboard.files')}: ${formatCount(format.files)}`
      },
    },
  }),
)

// Share of the views (image and thumbnail) per referrer, the split in the tooltip.
const referrerName = (referrer: string | null | undefined) => (referrer == null ? t('dashboard.other_referrers') : referrer || t('image.no_referrer'))
const referrersChart = computed(
  (): Highcharts.Options => ({
    chart: { type: 'pie', backgroundColor: 'transparent' },
    title: { text: undefined },
    credits: { enabled: false },
    series: [
      {
        type: 'pie',
        name: t('dashboard.views'),
        data: (stats.value?.referrers ?? []).map((row) => ({ name: referrerName(row.referrer), y: row.views + row.thumbnail_views })),
        dataLabels: { style: { color: themeColors.value.muted, textOutline: 'none' } },
      },
    ],
    tooltip: {
      formatter() {
        const row = stats.value?.referrers[this.index]
        if (!row) return false
        return `<b>${referrerName(row.referrer)}</b> (${(this.percentage ?? 0).toFixed(1)}%)<br/>${t('dashboard.views')}: ${formatCount(row.views)}<br/>${t('dashboard.thumbnail_views')}: ${formatCount(row.thumbnail_views)}`
      },
    },
  }),
)
</script>

<style scoped>
.dashboard {
  max-width: 1100px;
  margin: 0 auto;
  padding: 20px 16px;
  display: flex;
  flex-direction: column;
  gap: 24px;
}
h2,
h3 {
  margin: 0 0 8px;
}
.chart {
  height: 260px;
}
.filters {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 12px;
  padding-top: 8px;
}
.filters :deep(.p-select) {
  min-width: 10em;
}
.empty {
  color: var(--p-text-muted-color, #666);
}
.tiles {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 12px;
}
.tile {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 16px;
  border: 1px solid var(--p-content-border-color, #ddd);
  border-radius: 8px;
}
.tile small {
  color: var(--p-text-muted-color, #666);
}
.tile strong {
  font-size: 1.5rem;
}
.charts,
.tables {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 480px), 1fr));
  gap: 24px;
}
</style>
