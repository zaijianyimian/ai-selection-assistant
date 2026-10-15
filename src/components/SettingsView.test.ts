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
