import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { useAssistant } from '../stores/assistant'
import TranslationView from './TranslationView.vue'

vi.mock('../native', () => ({ desktop: true, command: vi.fn() }))

describe('translation result presentation', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('renders model content as escaped text and shows contextual terms', () => {
    const store = useAssistant()
    store.job = {
      revision: 1, status: 'success', original: 'JVM', source: 'manual', warning: null, error: null,
      result: { translation: '<script>alert(1)</script>', sourceLanguage: 'English', compatibilityFallback: false,
        terms: [{ original: 'JVM', translation: 'Java 虚拟机', domain: 'JVM', explanation: '执行 Java 字节码的运行环境。', example: null }] },
    }
    const view = mount(TranslationView, { props: { popup: false } })
    expect(view.findAll('script')).toHaveLength(0)
    expect(view.find('.translated-text').text()).toBe('<script>alert(1)</script>')
    expect(view.find('.term-card').text()).toContain('执行 Java 字节码')
    view.unmount()
  })

  it('shows loading and actionable failures instead of a stale result', async () => {
    const store = useAssistant()
    store.job.status = 'translating'
    const view = mount(TranslationView, { props: { popup: true } })
    expect(view.find('[role="status"]').exists()).toBe(true)
    store.job.status = 'error'
    store.job.error = { code: 'http_401', message: '身份验证失败，请检查 API Key。' }
    await nextTick()
    expect(view.find('[role="alert"]').text()).toContain('请检查 API Key')
    expect(view.find('[role="status"]').exists()).toBe(false)
    view.unmount()
  })
})
