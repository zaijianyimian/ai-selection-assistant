import { beforeEach, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import UsageLine from './UsageLine.vue'
import { snapshot, summary } from '../test/usage'
import { useUsage } from '../stores/usage'
beforeEach(() => setActivePinia(createPinia()))
it('renders operation counts, coverage, overlapping details and persistence errors', () => {
  const s = snapshot(); s.usage.total.tokens = '9007199254740993'; s.usage.total.unknown = 1
  s.usage.cachedInput.tokens = '20'; s.usage.reasoningOutput.tokens = '10'; s.persistenceIncomplete = true
  const view = mount(UsageLine, { props: { snapshot: s } })
  expect(view.text()).toContain('9,007,199,254,740,993'); expect(view.text()).toContain('未完整报告')
  expect(view.text()).toContain('子集，不另加总'); expect(view.text()).toContain('未完整保存'); view.unmount()
})
it('shows zero new local cache usage without replaying old operation numbers', () => {
  const s = snapshot(); s.source = 'local-cache'
  const view = mount(UsageLine, { props: { snapshot: s } })
  expect(view.text()).toContain('本次新增 0 Token'); expect(view.text()).not.toContain('130'); view.unmount()
})
it('uses current reset generation for results already visible in another window', () => {
  useUsage().accept(summary(2, 1))
  const view = mount(UsageLine, { props: { snapshot: snapshot() } })
  expect(view.text()).toContain('未纳入清空后的累计'); view.unmount()
})
it('distinguishes no sent request and missing usage', () => {
  const s = snapshot(); s.usage.attempts = 0
  const view = mount(UsageLine, { props: { snapshot: s } }); expect(view.text()).toContain('未发送模型请求'); view.unmount()
  s.usage.attempts = 1; s.usage.total.tokens = null
  const unknown = mount(UsageLine, { props: { snapshot: s } }); expect(unknown.text()).toContain('合计 未知'); unknown.unmount()
})
