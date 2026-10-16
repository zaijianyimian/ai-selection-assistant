import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { command, desktop } from '../native'
import { errorMessage, type AnalysisStatus, type Familiarity, type MemoryView } from '../types'

export const useMemory = defineStore('memory', () => {
  const view = ref<MemoryView>({ terms: [], total: 0, pending: 0, versions: [], activeVersion: 0, lastAutoAttempt: 0 })
  const status = ref<AnalysisStatus>({ running: false, message: '', error: null })
  const query = ref('')
  const mode = ref<'keyword' | 'fuzzy'>('keyword')
  const applied = ref({ query: '', mode: 'keyword' as 'keyword' | 'fuzzy' })
  const searching = ref(false)
  const offset = ref(0)
  const error = ref('')
  const working = ref(false)
  let listener: UnlistenFn | undefined
  let revision = 0
  async function refresh() {
    if (!desktop) return
    const current = ++revision
    searching.value = true
    const criteria = { ...applied.value }
    const page = offset.value
    try {
      const [next, analysis] = await Promise.all([
        command<MemoryView>('get_memory', { query: criteria.query, mode: criteria.mode, offset: page }),
        command<AnalysisStatus>('get_analysis_status'),
      ])
      if (current !== revision) return
      view.value = next; status.value = analysis; error.value = ''
      // A concurrent deletion may remove the last row of this page.
      if (!next.terms.length && offset.value > 0) { offset.value = 0; await refresh() }
    } catch (e) { if (current === revision) error.value = errorMessage(e) }
    finally { if (current === revision) searching.value = false }
  }
  async function search() {
    applied.value = { query: query.value.trim(), mode: mode.value }
    offset.value = 0
    await refresh()
  }
  async function initialize() {
    try {
      if (desktop && !listener) listener = await listen('memory-changed', () => { void refresh() })
      await refresh()
    } catch (e) { error.value = errorMessage(e) }
  }
  function dispose() { listener?.(); listener = undefined; revision++; searching.value = false }
  async function act(name: string, args?: Record<string, unknown>) {
    if (working.value) return
    working.value = true; error.value = ''
    try { await command(name, args); await refresh() }
    catch (e) { error.value = errorMessage(e) }
    finally { working.value = false }
  }
  async function feedback(id: number, value: Familiarity) { await act('set_term_feedback', { id, value }) }
  return { view, status, query, mode, applied, searching, search, offset, error, working, refresh, initialize, dispose, act, feedback }
})
