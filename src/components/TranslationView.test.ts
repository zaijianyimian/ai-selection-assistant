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
      requestOptions: { targetLanguage: '简体中文', explainTerms: true }, revision: 1, status: 'success', original: 'JVM', source: 'manual', warning: null, error: null,
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

describe('request display options', () => {
  beforeEach(() => setActivePinia(createPinia()))
  it.each([false, true])('keeps completed result language and terms in popup=%s', async popup => {
    const store = useAssistant()
    store.job = { requestOptions: { targetLanguage: '简体中文', explainTerms: true }, revision: 9, status: 'success', original: 'Agent', source: 'manual', warning: null, error: null,
      result: { translation: '智能体', sourceLanguage: 'English', compatibilityFallback: false, terms: [{ original: 'Agent', translation: '智能体', domain: 'Agent', explanation: '自主系统', example: null }] } }
    const view = mount(TranslationView, { props: { popup } })
    store.settings.targetLanguage = '日本語'; store.settings.explainTerms = false
    await nextTick()
    expect(view.find('.language-tag').text()).toBe('简体中文')
    expect(view.find('.translated-text').text()).toBe('智能体')
    expect(view.find('.term-card').exists()).toBe(true)
    expect(view.find('.section-heading').text()).not.toContain('术语解释已关闭')
    view.unmount()
  })
  it('retains options when defaults change during translation', async () => {
    const store = useAssistant()
    store.job.requestOptions = { targetLanguage: 'English', explainTerms: false }
    store.job.status = 'translating'
    const view = mount(TranslationView, { props: { popup: false } })
    store.settings.targetLanguage = '日本語'; store.settings.explainTerms = true
    await nextTick()
    store.job.status = 'success'; store.job.result = { translation: 'Hello', sourceLanguage: '中文', terms: [], compatibilityFallback: false }
    await nextTick()
    expect(view.find('.language-tag').text()).toBe('English')
    expect(view.find('.section-heading').text()).toContain('术语解释已关闭')
    view.unmount()
  })
  it('uses current defaults only when there is no result snapshot', async () => {
    const store = useAssistant()
    store.settings.targetLanguage = '日本語'
    const view = mount(TranslationView, { props: { popup: false } })
    expect(view.find('.language-tag').text()).toBe('日本語')
    view.unmount()
  })
})

it('explains Wayland placement limits while keeping popup controls', () => {
  setActivePinia(createPinia())
  const store = useAssistant(); store.platform = 'Wayland · 先复制'
  const view = mount(TranslationView, { props: { popup: true } })
  expect(view.find('.placement-hint').text()).toContain('不支持自动贴近选区')
  expect(view.find('.placement-hint').text()).toContain('本次运行')
  expect(view.find('#source-text').exists()).toBe(true)
  expect(view.find('.terms-section').exists()).toBe(true)
  view.unmount()
})
