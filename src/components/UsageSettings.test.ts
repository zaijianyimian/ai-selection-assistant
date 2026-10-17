import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import UsageSettings from './UsageSettings.vue'
import { summary } from '../test/usage'
import { useUsage } from '../stores/usage'
const native = vi.hoisted(() => ({ command: vi.fn() }))
vi.mock('../native', () => ({ desktop: true, command: native.command }))
describe('usage settings', () => {
  beforeEach(() => { setActivePinia(createPinia()); native.command.mockReset(); native.command.mockResolvedValue(summary()) })
  it('keeps reset outside model save, requires confirmation, and cancellation never writes', async () => {
    const view = mount(UsageSettings); await flushPromises()
    const button = (text: string) => view.findAll('button').find(b => b.text() === text)!
    await button('清空用量统计').trigger('click'); expect(view.text()).toContain('不会取消已发送')
    expect(native.command).not.toHaveBeenCalledWith('clear_usage', expect.anything())
    await button('取消清空').trigger('click'); expect(view.find('.usage-confirm').exists()).toBe(false)
    await button('清空用量统计').trigger('click')
    native.command.mockImplementation(async name => name === 'clear_usage' ? summary(2, 2) : summary(2, 3))
    await button('确认清空用量').trigger('click'); await flushPromises()
    expect(native.command).toHaveBeenCalledWith('clear_usage', { confirm: true })
    expect(native.command.mock.calls.some(([name]) => name === 'save_settings')).toBe(false)
    expect(view.find('.usage-confirm').exists()).toBe(false); view.unmount()
  })
  it('shows unknown coverage and storage gaps, supports filters and offline explanations', async () => {
    const s = summary(); s.incomplete = true; s.cumulative.total.tokens = null; s.cumulative.total.unknown = 1
    native.command.mockResolvedValue(s)
    const view = mount(UsageSettings); await flushPromises()
    expect(view.text()).toContain('保存缺口'); expect(view.text()).toContain('未知')
    await view.find('#usage-model').setValue('fixture-model'); await view.find('form').trigger('submit'); await flushPromises()
    expect(native.command).toHaveBeenCalledWith('get_usage', { query: expect.objectContaining({ model: 'fixture-model', offset: 0 }) })
    native.command.mockRejectedValue({ message: '不可用' }); await useUsage().refresh(); await flushPromises()
    expect(view.find('details').text()).toContain('不等于字数'); expect(view.find('details').text()).toContain('不是账户配额')
    expect(view.find('details a').exists()).toBe(false); view.unmount()
  })
})
