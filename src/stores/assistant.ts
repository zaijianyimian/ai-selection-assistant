import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { command, desktop } from '../native'
import { defaultSettings, errorMessage, type Job, type Settings, type SettingsView } from '../types'

export const useAssistant = defineStore('assistant', () => {
  const settings = ref(defaultSettings())
  const hasApiKey = ref(false)
  const warnings = ref<string[]>([])
  const input = ref('')
  const page = ref<'translate' | 'settings' | 'memory'>('translate')
  const platform = ref(desktop ? '正在检测桌面环境…' : '浏览器界面预览')
  const error = ref('')
  const pending = ref(false)
  const ready = ref(false)
  const job = ref<Job>({ revision: 0, status: 'idle', original: '', source: '', warning: null, result: null, error: null })
  const busy = computed(() => pending.value || ['capturing', 'translating'].includes(job.value.status))
  let listeners: UnlistenFn[] = []

  function acceptJob(next: Job) {
    if (next.revision < job.value.revision) return
    job.value = next
    if (next.original) input.value = next.original
  }
  function acceptSettings(view: SettingsView) {
    settings.value = view.settings
    hasApiKey.value = view.hasApiKey
    warnings.value = view.warnings
  }
  async function loadSettings() { acceptSettings(await command<SettingsView>('get_settings')) }

  async function initialize() {
    if (!desktop) { ready.value = true; return }
    try {
      listeners.push(await listen<Job>('job-changed', event => acceptJob(event.payload)))
      listeners.push(await listen('settings-changed', () => { void loadSettings().catch(e => { error.value = errorMessage(e) }) }))
      listeners.push(await listen('open-settings', () => { page.value = 'settings' }))
      await loadSettings()
      acceptJob(await command<Job>('get_job'))
      platform.value = await command<string>('platform_info')
    } catch (e) { error.value = errorMessage(e) }
    finally { ready.value = true }
  }

  async function translate(clipboard = false) {
    if (busy.value) return
    error.value = ''
    pending.value = true
    try {
      await command(clipboard ? 'translate_clipboard' : 'translate_text', clipboard ? undefined : { text: input.value })
      // Reconcile even if the webview was temporarily suspended during event delivery.
      acceptJob(await command<Job>('get_job'))
    } catch (e) { error.value = errorMessage(e) }
    finally { pending.value = false }
  }

  async function save(config: Settings, apiKey?: string, removeApiKey = false) {
    acceptSettings(await command<SettingsView>('save_settings', { config, apiKey: apiKey ?? null, removeApiKey }))
  }
  function dispose() { listeners.forEach(unlisten => unlisten()); listeners = [] }
  return { settings, hasApiKey, warnings, input, page, platform, error, ready, job, busy, initialize, dispose, translate, save, acceptJob }
})
