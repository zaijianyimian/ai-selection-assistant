import { beforeEach, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import SyncSettings from './SyncSettings.vue'
import { emptySync, useSync } from '../stores/sync'
const native = vi.hoisted(() => ({ command: vi.fn() }))
vi.mock('../native', () => ({ desktop: true, command: native.command }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))
beforeEach(() => { setActivePinia(createPinia()); native.command.mockReset(); native.command.mockImplementation(async (name: string) => name === 'get_sync' ? emptySync() : '连接验证成功') })
it('defaults off, requires consent and clears temporary passwords after testing', async () => {
  const v = mount(SyncSettings); await flushPromises()
  expect(v.text()).toContain('已断开')
  const login = () => v.findAll('button').find(b => b.text() === '登录并连接')!
  expect(login().attributes('disabled')).toBeDefined()
  await v.find('#sync-url').setValue('https://sync.example.com')
  await v.find('#sync-password').setValue('fixture-long-password')
  await v.findAll('button').find(b => b.text() === '测试同步连接')!.trigger('click'); await flushPromises()
  expect((v.find('#sync-password').element as HTMLInputElement).value).toBe('')
  expect(native.command).toHaveBeenCalledWith('test_sync_connection', expect.objectContaining({ password: 'fixture-long-password' }))
  expect(native.command.mock.calls.some(c => c[0] === 'connect_sync')).toBe(false)
  expect(JSON.stringify(useSync().$state)).not.toContain('fixture-long-password'); v.unmount()
})
it('clears password on keyring failure and preserves destination choices', async () => {
  native.command.mockImplementation(async (name: string) => { if (name === 'get_sync') return emptySync(); throw { message: '系统凭据存储不可用' } })
  const v = mount(SyncSettings); await flushPromises(); await v.find('#sync-url').setValue('https://new.example.com'); await v.find('#sync-password').setValue('fixture-long-password'); await v.find('input[type="checkbox"]').setValue(true)
  await v.findAll('button').find(b => b.text() === '登录并连接')!.trigger('click'); await flushPromises()
  expect(v.text()).toContain('系统凭据存储不可用'); expect((v.find('#sync-password').element as HTMLInputElement).value).toBe('')
  expect(native.command).toHaveBeenCalledWith('connect_sync', expect.objectContaining({ consent: true, replaceTarget: false, importLocal: false })); v.unmount()
})
it('renders escaped conflicts, resolves both choices and prevents duplicate actions', async () => {
  const state = { ...emptySync(), enabled: true, deviceId: 'device', status: 'conflict', pending: 1, conflicts: [{ id: 'conflict-1', field: 'theme', local: '<script>local</script>', remote: 'dark', revision: 4, code: 'field_changed' }] }
  let finish!: () => void
  native.command.mockImplementation(async (name: string) => { if (name === 'get_sync') return state; if (name === 'resolve_sync_conflict') return new Promise<void>(r => { finish = r }); return state })
  const v = mount(SyncSettings); await flushPromises(); expect(v.find('script').exists()).toBe(false); expect(v.text()).toContain('<script>local</script>')
  await v.findAll('button').find(b => b.text() === '保留本机并重试')!.trigger('click'); await v.findAll('button').find(b => b.text() === '采用远端')!.trigger('click')
  expect(native.command.mock.calls.filter(c => c[0] === 'resolve_sync_conflict')).toHaveLength(1); finish(); await flushPromises()
  await v.findAll('button').find(b => b.text() === '采用远端')!.trigger('click'); expect(native.command).toHaveBeenCalledWith('resolve_sync_conflict', { id: 'conflict-1', keepLocal: false }); finish(); await flushPromises(); v.unmount()
})
it('preserves edited server address when another window changes the connection', async () => {
  const v = mount(SyncSettings); await flushPromises(); await v.find('#sync-url').setValue('https://draft.example.com'); useSync().view = { ...emptySync(), url: 'https://saved.example.com' }; await flushPromises()
  expect((v.find('#sync-url').element as HTMLInputElement).value).toBe('https://draft.example.com'); expect(v.text()).toContain('当前编辑已保留'); v.unmount()
})
