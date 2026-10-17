import { beforeEach, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import ShortcutSettings from './ShortcutSettings.vue'
import TranslationView from './TranslationView.vue'
import { useShortcuts, type ShortcutStatus } from '../stores/shortcuts'
import { useAssistant } from '../stores/assistant'
const native = vi.hoisted(() => ({ command: vi.fn() }))
vi.mock('../native', () => ({ desktop: true, command: native.command }))
const initial = (): ShortcutStatus => ({ backend: 'gnome', capable: true, enabled: false, desired: 'Ctrl+Shift+Q', observed: null, status: 'not-configured', message: '尚未配置', command: '/usr/bin/ai-selection-assistant --translate-clipboard' })
beforeEach(() => { setActivePinia(createPinia()); native.command.mockReset(); useAssistant().ready = true; native.command.mockResolvedValue(initial()) })
it('only inspects on mount and enables after an explicit action', async () => {
  const view = mount(ShortcutSettings, { props: { draftShortcut: 'Ctrl+Shift+Q', working: false } }); await flushPromises()
  expect(native.command.mock.calls.every(c => c[0] === 'get_shortcut_status')).toBe(true)
  expect(view.text()).toContain('先 Ctrl+C'); expect(view.text()).toContain('--translate-clipboard')
  await view.findAll('button').find(b => b.text() === '启用系统快捷键')!.trigger('click'); await flushPromises()
  expect(native.command).toHaveBeenCalledWith('manage_shortcut', { action: 'enable', expectedShortcut: 'Ctrl+Shift+Q' }); view.unmount()
})
it('requires saving a dirty shortcut before enabling and does not send unsaved keys', async () => {
  const view = mount(ShortcutSettings, { props: { draftShortcut: 'Alt+Shift+T', working: false } }); await flushPromises()
  const enable = view.findAll('button').find(b => b.text() === '启用系统快捷键')!
  expect(enable.attributes('disabled')).toBeDefined(); expect(view.text()).toContain('请先保存')
  await enable.trigger('click'); expect(native.command.mock.calls.some(c => c[0] === 'manage_shortcut')).toBe(false); view.unmount()
})
it('prevents duplicate operations and reports errors without success', async () => {
  let reject!: (value: unknown) => void
  native.command.mockImplementation(name => name === 'manage_shortcut' ? new Promise((_r, j) => { reject = j }) : Promise.resolve(initial()))
  const view = mount(ShortcutSettings, { props: { draftShortcut: 'Ctrl+Shift+Q', working: false } }); await flushPromises()
  const enable = view.findAll('button').find(b => b.text() === '启用系统快捷键')!
  await enable.trigger('click'); await enable.trigger('click')
  expect(native.command.mock.calls.filter(c => c[0] === 'manage_shortcut')).toHaveLength(1)
  reject({ message: '系统快捷键被占用' }); await flushPromises()
  expect(view.text()).toContain('系统快捷键被占用'); expect(view.text()).not.toContain('启用成功'); view.unmount()
})
it.each(['configured', 'external-change', 'conflict', 'unavailable', 'recovery-required'] as const)('shows %s without writing configuration', async status => {
  native.command.mockResolvedValue({ ...initial(), status, enabled: true, observed: '<Alt>a', message: '请检查系统设置' })
  const view = mount(ShortcutSettings, { props: { draftShortcut: 'Ctrl+Shift+Q', working: false } }); await flushPromises()
  expect(view.text()).toContain('请检查系统设置'); expect(view.text()).toContain('<Alt>a')
  expect(native.command.mock.calls.every(c => c[0] === 'get_shortcut_status')).toBe(true)
  if (status === 'recovery-required') expect(view.text()).toContain('恢复快捷键操作')
  view.unmount()
})
it('uses copy-first guidance and the actual system binding on the translation page', async () => {
  const store = useShortcuts(); store.state = { ...initial(), status: 'external-change', observed: '<Alt>a' }
  useAssistant().platform = 'Wayland · 先复制'
  const view = mount(TranslationView, { props: { popup: false } })
  expect(view.find('.shortcut-strip').text()).toContain('先复制文字')
  expect(view.find('.shortcut-strip').text()).toContain('<Alt>a'); view.unmount()
})
it('discards a stale refresh result after a newer operation', async () => {
  let finish!: (s: ShortcutStatus) => void
  native.command.mockImplementationOnce(() => new Promise(r => { finish = r })).mockResolvedValue({ ...initial(), enabled: true, status: 'configured' })
  const store = useShortcuts(); const stale = store.refresh(); await store.manage('enable', initial().desired)
  finish(initial()); await stale
  expect(store.state?.status).toBe('configured')
})
