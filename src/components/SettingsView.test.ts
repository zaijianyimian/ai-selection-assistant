import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { useAssistant } from '../stores/assistant'
import { defaultSettings } from '../types'
import SettingsView from './SettingsView.vue'

const native = vi.hoisted(() => ({ command: vi.fn() }))
vi.mock('../native', () => ({ desktop: true, command: native.command }))

describe('credential input lifecycle', () => {
  beforeEach(() => { setActivePinia(createPinia()); native.command.mockReset() })

  it('keeps memory and personalization independent and stops automation when memory is paused', async () => {
    const store = useAssistant(); store.ready = true
    const view = mount(SettingsView)
    expect(view.find('#auto-analyze').attributes('disabled')).toBeDefined()
    await view.find('#remember-terms').setValue(true)
    await view.find('#auto-analyze').setValue(true)
    expect((view.find('#personalize').element as HTMLInputElement).checked).toBe(false)
    await view.find('#personalize').setValue(true)
    await view.find('#remember-terms').setValue(false)
    expect((view.find('#auto-analyze').element as HTMLInputElement).checked).toBe(false)
    expect((view.find('#personalize').element as HTMLInputElement).checked).toBe(true)
    view.unmount()
  })

  it('sends only newly typed key and clears it after saving', async () => {
    const store = useAssistant()
    store.ready = true
    store.hasApiKey = true
    native.command.mockResolvedValue({ settings: defaultSettings(), hasApiKey: true, warnings: [] })
    const view = mount(SettingsView)
    expect((view.find('#api-key').element as HTMLInputElement).value).toBe('')
    await view.find('#api-key').setValue('fixture-key')
    await view.find('form').trigger('submit')
    await flushPromises()
    expect(native.command).toHaveBeenCalledWith('save_settings', expect.objectContaining({ apiKey: 'fixture-key', removeApiKey: false }))
    expect((view.find('#api-key').element as HTMLInputElement).value).toBe('')
    expect(JSON.stringify(store.$state)).not.toContain('fixture-key')
    view.unmount()
  })

  it('clears temporary credentials after a failed connection test', async () => {
    const store = useAssistant()
    store.ready = true
    native.command.mockRejectedValue({ code: 'http_401', message: '身份验证失败' })
    const view = mount(SettingsView)
    await view.find('#api-key').setValue('fixture-key')
    const testButton = view.findAll('button').find(button => button.text() === '测试连接')!
    await testButton.trigger('click')
    await flushPromises()
    expect(view.find('[role="alert"]').text()).toContain('身份验证失败')
    expect((view.find('#api-key').element as HTMLInputElement).value).toBe('')
    view.unmount()
  })
})

describe('settings drafts and synchronization', () => {
  beforeEach(() => { setActivePinia(createPinia()); native.command.mockReset() })
  it('preserves dirty drafts and reloads latest saved settings explicitly', async () => {
    const store = useAssistant(); store.ready = true
    const view = mount(SettingsView)
    await view.find('#model').setValue('unsaved-model')
    await view.find('#api-key').setValue('fixture-temporary')
    store.settings = { ...store.settings, model: 'external-model', theme: 'dark' }
    await flushPromises()
    expect((view.find('#model').element as HTMLInputElement).value).toBe('unsaved-model')
    expect((view.find('#api-key').element as HTMLInputElement).value).toBe('fixture-temporary')
    expect(view.find('[data-testid="settings-external-update"]').exists()).toBe(true)
    await view.find('[data-testid="settings-external-update"] button').trigger('click')
    expect((view.find('#model').element as HTMLInputElement).value).toBe('external-model')
    expect((view.find('#api-key').element as HTMLInputElement).value).toBe('')
    expect(view.find('[data-testid="settings-external-update"]').exists()).toBe(false)
    expect(JSON.stringify(store.$state)).not.toContain('fixture-temporary')
    view.unmount()
  })
  it('updates clean forms automatically and protects key deletion intent', async () => {
    const store = useAssistant(); store.hasApiKey = true
    const view = mount(SettingsView)
    store.settings = { ...store.settings, model: 'clean-update' }
    await flushPromises()
    expect((view.find('#model').element as HTMLInputElement).value).toBe('clean-update')
    expect(view.find('[data-testid="settings-external-update"]').exists()).toBe(false)
    await view.find('input[type="checkbox"]').setValue(true)
    store.settings = { ...store.settings, model: 'second-update' }
    await flushPromises()
    expect((view.find('#model').element as HTMLInputElement).value).toBe('clean-update')
    expect(view.find('[data-testid="settings-external-update"]').exists()).toBe(true)
    view.unmount()
  })
  it.each(['before', 'after'])('accepts own save event %s response without false conflicts', async timing => {
    const store = useAssistant(); store.ready = true
    const accepted = { ...defaultSettings(), model: 'saved-model' }
    native.command.mockImplementation(async () => {
      if (timing === 'before') store.settings = { ...accepted }
      return { settings: accepted, hasApiKey: false, warnings: [] }
    })
    const view = mount(SettingsView)
    await view.find('#model').setValue('saved-model')
    await view.find('form').trigger('submit')
    await flushPromises()
    if (timing === 'after') { store.settings = { ...accepted }; await flushPromises() }
    expect(view.find('[data-testid="settings-external-update"]').exists()).toBe(false)
    expect((view.find('#model').element as HTMLInputElement).value).toBe('saved-model')
    view.unmount()
  })
  it('retains non-secret edits on failed save and clears temporary key', async () => {
    const store = useAssistant(); store.ready = true
    native.command.mockRejectedValue({ message: '保存失败' })
    const view = mount(SettingsView)
    await view.find('#model').setValue('retry-model')
    await view.find('#api-key').setValue('fixture-key')
    await view.find('form').trigger('submit'); await flushPromises()
    expect((view.find('#model').element as HTMLInputElement).value).toBe('retry-model')
    expect((view.find('#api-key').element as HTMLInputElement).value).toBe('')
    expect(view.find('[role="alert"]').text()).toContain('保存失败')
    view.unmount()
  })
  it('lets the user explicitly save a retained draft after an external update', async () => {
    const store = useAssistant(); store.ready = true
    native.command.mockImplementation(async (_name: string, args: { config: ReturnType<typeof defaultSettings> }) => ({ settings: args.config, hasApiKey: false, warnings: [] }))
    const view = mount(SettingsView)
    await view.find('#model').setValue('retained-model')
    store.settings = { ...store.settings, model: 'external-model' }
    await flushPromises()
    expect(view.find('[data-testid="settings-external-update"]').exists()).toBe(true)
    await view.find('form').trigger('submit'); await flushPromises()
    expect(native.command).toHaveBeenCalledWith('save_settings', expect.objectContaining({ config: expect.objectContaining({ model: 'retained-model' }) }))
    expect(view.find('[data-testid="settings-external-update"]').exists()).toBe(false)
    expect((view.find('#model').element as HTMLInputElement).value).toBe('retained-model')
    view.unmount()
  })
  it('treats a temporary key alone as a dirty draft', async () => {
    const store = useAssistant()
    const view = mount(SettingsView)
    await view.find('#api-key').setValue('fixture-temporary-only')
    store.settings = { ...store.settings, model: 'external-model' }
    await flushPromises()
    expect((view.find('#model').element as HTMLInputElement).value).toBe(defaultSettings().model)
    expect((view.find('#api-key').element as HTMLInputElement).value).toBe('fixture-temporary-only')
    expect(view.find('[data-testid="settings-external-update"]').exists()).toBe(true)
    view.unmount()
  })
  it('does not hide an external update received while saving a local draft', async () => {
    const store = useAssistant(); store.ready = true
    let resolve!: (x: unknown) => void
    native.command.mockReturnValue(new Promise(r => { resolve = r }))
    const view = mount(SettingsView)
    await view.find('#model').setValue('local-save')
    await view.find('form').trigger('submit')
    store.settings = { ...store.settings, model: 'another-window' }
    await flushPromises()
    resolve({ settings: { ...defaultSettings(), model: 'local-save' }, hasApiKey: false, warnings: [] })
    await flushPromises()
    expect(view.find('[data-testid="settings-external-update"]').exists()).toBe(true)
    await view.find('[data-testid="settings-external-update"] button').trigger('click')
    expect((view.find('#model').element as HTMLInputElement).value).toBe('another-window')
    view.unmount()
  })
})
