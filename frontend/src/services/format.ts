// Human-readable byte count in binary units, e.g. 1536 -> "1.5 KiB".
export const formatBytes = (bytes: number): string => {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(1)} ${units[unit]}`
}

const relativeTimeUnits: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
]

// How long ago (or from now) an ISO date is, in its largest whole unit, e.g. "3 days ago".
export const timeAgo = (date: string, now: Date = new Date()): string => {
  const seconds = Math.round((Date.parse(date) - now.getTime()) / 1000)
  const format = new Intl.RelativeTimeFormat('en', { numeric: 'auto' })
  for (const [unit, unitSeconds] of relativeTimeUnits) {
    if (Math.abs(seconds) >= unitSeconds) return format.format(Math.trunc(seconds / unitSeconds), unit)
  }
  return format.format(seconds, 'second')
}

// Resolves a possibly relative link against the current origin.
export const absoluteUrl = (link: string): string => (link ? new URL(link, window.location.origin).href : '')

// An ISO date as local "2026-10-15 14:00:00".
export const formatDate = (date: string): string => new Date(date).toLocaleString('sv-SE')

// Comma separated entries, trimmed, empty ones dropped.
export const splitList = (text: string): string[] =>
  text
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean)
