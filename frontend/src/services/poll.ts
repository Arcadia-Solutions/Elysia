import { onUnmounted, type Ref } from 'vue'

// Returns a refresh that loads `progress` and keeps polling it once a second
// while a run goes, calling `onEnd` when a run it saw going stops. Polling
// stops with the calling component.
export function pollProgress<T extends { running: boolean }>(
  fetch: () => Promise<{ data: T }>,
  progress: Ref<T | undefined>,
  onEnd: (progress: T) => void,
): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined
  const refresh = () => {
    clearTimeout(timer)
    fetch().then(({ data }) => {
      const wasRunning = progress.value?.running
      progress.value = data
      if (data.running) timer = setTimeout(refresh, 1000)
      else if (wasRunning) onEnd(data)
    })
  }
  onUnmounted(() => clearTimeout(timer))
  return refresh
}
