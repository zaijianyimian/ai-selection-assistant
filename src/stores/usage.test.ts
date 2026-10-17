import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useUsage } from './usage'
import { summary } from '../test/usage'
const native = vi.hoisted(() => ({ command: vi.fn(), stop: vi.fn(), handler: undefined as (() => void) | undefined }))
vi.mock('../native', () => ({ desktop: true, command: native.command }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async (_name, handler) => { native.handler = handler; return native.stop }) }))
describe('local usage state', () => {
  beforeEach(() => { setActivePinia(createPinia()); native.command.mockReset(); native.stop.mockReset() })
  it('rejects old generations and revisions', () => {
    const store = useUsage(); store.accept(summary(2, 4)); store.accept(summary(1, 100)); store.accept(summary(2, 3))
    expect(store.view?.generation).toBe(2); expect(store.view?.revision).toBe(4)
  })
  it('keeps the latest filter response and does not perform writes on view/events', async () => {
    const store = useUsage(); let old!: (v: unknown) => void
    native.command.mockReturnValueOnce(new Promise(resolve => { old = resolve })).mockResolvedValue(summary(1, 2))
    const first = store.refresh(); store.query.model = 'new-model'; await store.refresh()
    old(summary(1, 1)); await first
    expect(store.view?.revision).toBe(2)
    await store.initialize(); native.handler?.(); await Promise.resolve(); store.dispose()
    expect(native.stop).toHaveBeenCalled(); expect(native.command.mock.calls.every(([name]) => name === 'get_usage')).toBe(true)
  })
  it('resets using confirmation, invalidates stale reads and retains error information', async () => {
    const store = useUsage(); let stale!: (v: unknown) => void
    native.command.mockReturnValueOnce(new Promise(resolve => { stale = resolve }))
    const read = store.refresh()
    native.command.mockImplementation(async name => name === 'clear_usage' ? summary(2, 3) : summary(2, 4))
    expect(await store.clear()).toBe(true)
    stale(summary(1, 100)); await read
    expect(store.view?.generation).toBe(2)
    expect(native.command).toHaveBeenCalledWith('clear_usage', { confirm: true })
    native.command.mockRejectedValue({ message: '磁盘满' }); expect(await store.clear()).toBe(false)
    expect(store.error).toBe('磁盘满'); expect(store.view?.generation).toBe(2)
  })
})
