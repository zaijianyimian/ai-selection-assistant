import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { useAssistant } from '../stores/assistant'
import MemoryView from './MemoryView.vue'
import type { MemoryView as Snapshot } from '../types'
const native = vi.hoisted(() => ({ command: vi.fn() }))
vi.mock('../native', () => ({ desktop: true, command: native.command }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))
const snapshot = (): Snapshot => ({ terms: [{ id: 42, original: 'Agent', translation: '智能体', domain: 'Agent', senseSummary: '自主执行任务的软件系统', language: 'English', count: 3, lastSeen: 1000, familiarity: 'unknown' }], total: 1, pending: 3, versions: [{ id: 1, createdAt: 1000, profile: { topics: [{ domain: 'Agent', evidenceIds: [42] }] } }], activeVersion: 1, lastAutoAttempt: 0 })
describe('memory user controls', () => {
  beforeEach(() => {
    setActivePinia(createPinia()); native.command.mockReset()
    native.command.mockImplementation(async (name: string) => name === 'get_memory' ? snapshot() : name === 'get_analysis_status' ? { running: false, message: '', error: null } : undefined)
    const assistant = useAssistant(); assistant.ready = true; assistant.settings.rememberTerms = true
  })
  it('requires explicit deletion confirmation and targets stable term ID', async () => {
    const view = mount(MemoryView); await flushPromises()
    expect(view.text()).toContain('自主执行任务的软件系统')
    await view.findAll('button').find(b => b.text() === '删除')!.trigger('click')
    expect(native.command).not.toHaveBeenCalledWith('delete_memory', expect.anything())
    await view.findAll('button').find(b => b.text() === '取消')!.trigger('click')
    expect(view.text()).not.toContain('确认删除此术语')
    await view.findAll('button').find(b => b.text() === '删除')!.trigger('click')
    await view.findAll('button').find(b => b.text() === '确认删除')!.trigger('click'); await flushPromises()
    expect(native.command).toHaveBeenCalledWith('delete_memory', { id: 42 })
    view.unmount()
  })
  it('sends explicit feedback and starts manual analysis without source text', async () => {
    const view = mount(MemoryView); await flushPromises()
    await view.find('#familiarity-42').setValue('known'); await flushPromises()
    expect(native.command).toHaveBeenCalledWith('set_term_feedback', { id: 42, value: 'known' })
    await view.findAll('button').find(b => b.text() === '分析新增记忆')!.trigger('click'); await flushPromises()
    expect(native.command).toHaveBeenCalledWith('analyze_memory', undefined)
    expect(view.text()).toContain('证据 #42')
    view.unmount()
  })
})

it('selects local fuzzy mode and searches definitions without hiding memory controls', async () => {
  setActivePinia(createPinia()); native.command.mockReset()
  native.command.mockImplementation(async (name: string) => name === 'get_memory' ? snapshot() : { running: false, message: '', error: null })
  const view = mount(MemoryView); await flushPromises()
  await view.find('#term-search').setValue('自主规划')
  await view.find('#term-search-mode').setValue('fuzzy'); await flushPromises()
  expect(native.command).toHaveBeenCalledWith('get_memory', { query: '自主规划', mode: 'fuzzy', offset: 0 })
  expect(view.text()).toContain('不下载模型、不联网')
  expect(view.find('#familiarity-42').exists()).toBe(true)
  expect(view.find('#term-search').attributes('placeholder')).toContain('词义')
  view.unmount()
})

it('renders pending, empty and failed keyword searches with a retryable form', async () => {
  setActivePinia(createPinia()); native.command.mockReset()
  let resolve!: (value: Snapshot) => void
  native.command.mockImplementation((name: string) => name === 'get_memory' ? new Promise(r => { resolve = r }) : Promise.resolve({ running: false, message: '', error: null }))
  const view = mount(MemoryView); await flushPromises()
  expect(view.text()).toContain('正在搜索本地术语')
  expect(native.command).toHaveBeenCalledWith('get_memory', { query: '', mode: 'keyword', offset: 0 })
  resolve({ ...snapshot(), terms: [], total: 0 }); await flushPromises()
  expect(view.text()).toContain('暂无匹配术语')
  native.command.mockImplementation(async (name: string) => { if (name === 'get_memory') throw { message: '本地索引不可用' }; return { running: false, message: '', error: null } })
  await view.find('form').trigger('submit'); await flushPromises()
  expect(view.find('[role="alert"]').text()).toContain('本地索引不可用')
  expect(view.find('form button[type="submit"]').attributes('disabled')).toBeUndefined()
  view.unmount()
})
