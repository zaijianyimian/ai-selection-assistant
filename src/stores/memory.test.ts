import { beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useMemory } from './memory'
const native = vi.hoisted(() => ({ command: vi.fn() }))
vi.mock('../native', () => ({ desktop: true, command: native.command }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))
const snapshot = (name = '', count = 0) => ({ terms: name ? [{ id: 1, original: name }] : [], total: count, pending: 0, versions: [], activeVersion: 0, lastAutoAttempt: 0 })
beforeEach(() => { setActivePinia(createPinia()); native.command.mockReset() })
it('submits mode and query together and pages using committed conditions', async () => {
  native.command.mockImplementation(async (name: string) => name === 'get_memory' ? snapshot('Agent', 100) : { running: false, message: '', error: null })
  const memory = useMemory()
  memory.query = '规划'; memory.mode = 'fuzzy'; memory.offset = 30
  await memory.search()
  expect(memory.offset).toBe(0)
  expect(native.command).toHaveBeenCalledWith('get_memory', { query: '规划', mode: 'fuzzy', offset: 0 })
  memory.query = '未提交'; memory.offset = 30
  await memory.refresh()
  expect(native.command).toHaveBeenLastCalledWith('get_analysis_status')
  expect(native.command).toHaveBeenCalledWith('get_memory', { query: '规划', mode: 'fuzzy', offset: 30 })
})
it('keeps newer results and search state when an old response arrives late', async () => {
  let resolve!: (x: unknown) => void
  native.command.mockImplementation((name: string, args: { query: string }) => name === 'get_memory'
    ? (args.query === 'old' ? new Promise(r => { resolve = r }) : Promise.resolve(snapshot('new', 1)))
    : Promise.resolve({ running: false, message: '', error: null }))
  const memory = useMemory(); memory.query = 'old'
  const old = memory.search(); memory.query = 'new'; memory.mode = 'fuzzy'
  await memory.search()
  resolve(snapshot('old', 1)); await old
  expect(memory.view.terms[0]?.original).toBe('new')
  expect(memory.applied).toEqual({ query: 'new', mode: 'fuzzy' })
  expect(memory.searching).toBe(false)
})
it('resets an emptied last page and exposes recoverable search errors', async () => {
  native.command.mockImplementation(async (name: string, args: { offset: number }) => name === 'get_memory' ? (args.offset ? snapshot() : snapshot('remaining', 1)) : { running: false, message: '', error: null })
  const memory = useMemory(); memory.offset = 30
  await memory.refresh()
  expect(memory.offset).toBe(0); expect(memory.view.terms[0]?.original).toBe('remaining')
  native.command.mockRejectedValue({ message: '本地索引不可用' })
  await memory.search()
  expect(memory.error).toContain('本地索引不可用'); expect(memory.searching).toBe(false)
})
