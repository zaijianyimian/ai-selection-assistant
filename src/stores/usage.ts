import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { command, desktop } from '../native'
import { errorMessage, type UsageQuery, type UsageSummary } from '../types'

export const useUsage = defineStore('usage', () => {
  const view = ref<UsageSummary | null>(null)
  const query = ref<UsageQuery>({ offset: 0, limit: 20 })
  const error = ref('')
  const loading = ref(false)
  const resetting = ref(false)
  const pending = computed(() => loading.value || resetting.value)
  let listener: UnlistenFn | undefined
  let disposed = false
  let request = 0
  function accept(next: UsageSummary) {
    if (!next || typeof next.generation !== 'number') return
    if (view.value && (next.generation < view.value.generation || (next.generation === view.value.generation && next.revision < view.value.revision))) return
    view.value = next
  }
  async function refresh() {
    if (!desktop) return
    const revision = ++request
    loading.value = true
    try {
      const next = await command<UsageSummary>('get_usage', { query: { ...query.value } })
      if (revision === request) { accept(next); error.value = '' }
    } catch (e) { if (revision === request) error.value = errorMessage(e) }
    finally { if (revision === request) loading.value = false }
  }
  async function initialize() {
    disposed = false
    if (desktop && !listener) {
      try {
        const stop = await listen('usage-changed', () => { void refresh() })
        if (disposed) { stop(); return }; listener = stop
      } catch (e) { error.value = errorMessage(e) }
    }
    await refresh()
  }
  async function clear() {
    if (resetting.value) return false
    ++request
    resetting.value = true; error.value = ''
    try {
      const next = await command<UsageSummary>('clear_usage', { confirm: true })
      request++; query.value.offset = 0; accept(next)
      await refresh()
      return true
    } catch (e) { error.value = errorMessage(e); return false }
    finally { resetting.value = false }
  }
  function dispose() { disposed = true; listener?.(); listener = undefined; request++; loading.value = false }
  return { view, query, error, pending, accept, refresh, initialize, clear, dispose }
})
