import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useAssistant } from './assistant'
import { defaultSettings, type Job } from '../types'

const native = vi.hoisted(() => ({ command: vi.fn(), handlers: new Map<string, (event: { payload: unknown }) => void>() }))
vi.mock('../native', () => ({ desktop: true, command: native.command }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
  native.handlers.set(name, handler)
  return () => native.handlers.delete(name)
}) }))

const idle = (): Job => ({ requestOptions: null, revision: 0, status: 'idle', original: '', source: '', warning: null, result: null, error: null })

describe('native job synchronization', () => {
  beforeEach(() => { setActivePinia(createPinia()); native.command.mockReset(); native.handlers.clear() })

  it('uses the cache by default and bypasses only on explicit refresh', async () => {
    const store = useAssistant(); store.input = 'Agent'
    native.command.mockImplementation(async name => name === 'get_job' ? idle() : undefined)
    await store.translate()
    expect(native.command).toHaveBeenCalledWith('translate_text', { text: 'Agent' })
    await store.translate(false, true)
    expect(native.command).toHaveBeenCalledWith('translate_text', { text: 'Agent', bypassCache: true })
    await store.translate(true)
    expect(native.command).toHaveBeenCalledWith('translate_clipboard', undefined)
  })

  it('keeps a newer result when an older snapshot arrives after an event', async () => {
    const store = useAssistant()
    const completed: Job = { ...idle(), requestOptions: { targetLanguage: 'English', explainTerms: false }, revision: 5, status: 'success', original: 'source', result: { translation: '译文', sourceLanguage: 'English', terms: [], compatibilityFallback: false } }
    native.command.mockImplementation(async (name: string) => {
      if (name === 'get_settings') return { settings: defaultSettings(), hasApiKey: true, warnings: [] }
      if (name === 'get_job') {
        native.handlers.get('job-changed')?.({ payload: completed })
        return { ...idle(), revision: 4, status: 'translating' }
      }
      return 'Windows'
    })
    await store.initialize()
    expect(store.job.status).toBe('success')
    expect(store.job.result?.translation).toBe('译文')
    expect(store.input).toBe('source')
    expect(store.job.requestOptions).toEqual({ targetLanguage: 'English', explainTerms: false })
    store.dispose()
    expect(native.handlers.size).toBe(0)
  })

  it('blocks duplicate submissions and restores controls after IPC failure', async () => {
    const store = useAssistant()
    let reject!: (error: unknown) => void
    native.command.mockImplementation(() => new Promise((_resolve, fail) => { reject = fail }))
    store.input = 'Hello'
    const request = store.translate()
    await store.translate()
    expect(native.command).toHaveBeenCalledTimes(1)
    expect(store.busy).toBe(true)
    reject({ code: 'network', message: '连接失败' })
    await request
    expect(store.busy).toBe(false)
    expect(store.error).toBe('连接失败')
    expect(store.input).toBe('Hello')
  })

  it('recovers an active backend job during window initialization', async () => {
    const store = useAssistant()
    native.command.mockImplementation(async (name: string) => {
      if (name === 'get_settings') return { settings: defaultSettings(), hasApiKey: false, warnings: ['快捷键冲突'] }
      if (name === 'get_job') return { ...idle(), requestOptions: { targetLanguage: '日本語', explainTerms: true }, revision: 8, status: 'capturing' }
      return 'Wayland'
    })
    await store.initialize()
    expect(store.busy).toBe(true)
    expect(store.warnings).toEqual(['快捷键冲突'])
    expect(store.ready).toBe(true)
    expect(store.job.requestOptions?.targetLanguage).toBe('日本語')
  })
})
