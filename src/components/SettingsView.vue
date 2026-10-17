<script setup lang="ts">
import { computed, onUnmounted, reactive, ref, watch } from 'vue'
import { useAssistant } from '../stores/assistant'
import { command } from '../native'
import { errorMessage, settingsKey, type Settings, type ConnectionTest } from '../types'
import Icon from './Icon.vue'
import SyncSettings from './SyncSettings.vue'
import ShortcutSettings from './ShortcutSettings.vue'
import UsageSettings from './UsageSettings.vue'
import UsageLine from './UsageLine.vue'
import type { UsageSnapshot } from '../types'
import { useShortcuts } from '../stores/shortcuts'

const store = useAssistant()
const shortcuts = useShortcuts()
const form = reactive({ ...store.settings })
const apiKey = ref('')
const removeKey = ref(false)
const working = ref<'save' | 'test' | null>(null)
const message = ref('')
const failure = ref('')
const testUsage = ref<UsageSnapshot | null>(null)
const baseline = ref({ ...store.settings })
const externalUpdate = ref<Settings | null>(null)
const submitted = ref<Settings | null>(null)
let externalVersion = 0
const dirty = computed(() => settingsKey(form) !== settingsKey(baseline.value) || !!apiKey.value || removeKey.value)
watch(() => store.settings, config => {
  if (submitted.value && settingsKey(config) === settingsKey(submitted.value)) return
  if (!dirty.value) {
    Object.assign(form, config); baseline.value = { ...config }; externalUpdate.value = null
  } else if (settingsKey(config) !== settingsKey(baseline.value)) {
    externalVersion++
    externalUpdate.value = { ...config }
  }
}, { flush: 'sync' })
function reload() {
  Object.assign(form, store.settings)
  baseline.value = { ...store.settings }
  externalUpdate.value = null; apiKey.value = ''; removeKey.value = false
  message.value = ''; failure.value = ''
}
watch(() => form.rememberTerms, enabled => { if (!enabled) form.autoAnalyze = false })
onUnmounted(() => { apiKey.value = '' })

async function save() {
  if (shortcuts.pending) return
  working.value = 'save'; message.value = ''; failure.value = ''
  const externalAtSubmit = externalVersion
  try {
    submitted.value = { ...form }
    const accepted = await store.save({ ...form }, apiKey.value.trim() || undefined, removeKey.value)
    Object.assign(form, accepted.settings)
    baseline.value = { ...accepted.settings }
    if (externalAtSubmit === externalVersion || (externalUpdate.value && settingsKey(externalUpdate.value) === settingsKey(accepted.settings))) externalUpdate.value = null
    message.value = '设置已保存，立即生效。'
    removeKey.value = false
  } catch (e) { failure.value = errorMessage(e) }
  finally { apiKey.value = ''; working.value = null; submitted.value = null }
}
async function test() {
  testUsage.value = null
  working.value = 'test'; message.value = ''; failure.value = ''
  try {
    const result = await command<ConnectionTest>('test_connection', { config: { ...form }, apiKey: removeKey.value ? '' : (apiKey.value.trim() || null) })
    testUsage.value = result.usage ?? null
    if (result.error) { failure.value = result.error.message; return }
    message.value = `${result.message} 耗时 ${(result.elapsedMs / 1000).toFixed(2)} 秒。${result.compatibilityFallback ? ' 已自动调整输出格式或 Temperature 以兼容服务。' : ''}当前测试不会保存设置。`
  } catch (e) { failure.value = errorMessage(e) }
  finally { apiKey.value = ''; working.value = null }
}
</script>

<template>
  <section class="settings-view"><div class="page-heading"><div><div class="eyebrow">MAKE IT YOURS</div><h1>连接你的模型<span>。</span></h1><p>选择熟悉的服务，让翻译按你的方式工作。</p></div></div>
    <form class="settings-form" @submit.prevent="save">
      <div v-if="externalUpdate" class="inline-message warning" role="status" data-testid="settings-external-update"><span>设置已在其他窗口更新，当前未保存的编辑已保留。</span><button class="button secondary" type="button" :disabled="!!working" @click="reload">重新加载已保存设置</button></div>
      <section class="settings-card"><div class="settings-card-title"><Icon name="spark" /><div><h2>模型服务</h2><p>支持 OpenAI-compatible Chat Completions API</p></div></div>
        <div class="field"><label for="base-url">Base URL</label><input id="base-url" v-model="form.baseUrl" type="url" required placeholder="https://api.example.com/v1" :disabled="!!working" autocomplete="off" spellcheck="false" /><p class="field-help">支持服务根地址、带 /v1 的路径或完整 /chat/completions 地址。HTTPS 必填；本机服务可使用 HTTP。</p></div>
        <div class="field"><label for="api-key">API Key <span class="field-status">{{ store.hasApiKey ? '已保存在系统凭据存储' : '尚未配置 · 无认证本机模型可留空' }}</span></label><input id="api-key" v-model="apiKey" type="password" :placeholder="store.hasApiKey ? '留空以保留现有 Key' : '输入服务商提供的 API Key'" autocomplete="new-password" spellcheck="false" :disabled="!!working || removeKey" /><p class="field-help">仅用于本次测试或替换密钥。保存或测试后清空输入，已保存密钥不会回传到界面。</p><label v-if="store.hasApiKey" class="checkbox-label"><input v-model="removeKey" type="checkbox" :disabled="!!working" @change="apiKey = ''" />保存时删除现有 API Key</label></div>
        <div class="field"><label for="model">Model</label><input id="model" v-model="form.model" required maxlength="200" placeholder="输入模型名称" :disabled="!!working" autocomplete="off" spellcheck="false" /></div>
        <div class="form-row"><div class="field"><label for="temperature">Temperature</label><input id="temperature" v-model.number="form.temperature" type="number" min="0" max="2" step="0.1" required :disabled="!!working" /><p class="field-help">默认 0.2，兼顾准确性与自然表达。</p></div><div class="field"><label for="timeout">请求超时（秒）</label><input id="timeout" v-model.number="form.timeoutSecs" type="number" min="5" max="180" required :disabled="!!working" /><p class="field-help">5–180 秒，包含兼容模式的重试。</p></div></div>
        <div class="field"><label for="output-mode">输出兼容模式</label><select id="output-mode" v-model="form.outputMode" :disabled="!!working"><option value="auto">自动 · 按服务能力选择 JSON</option><option value="json">JSON 模式 · json_object</option><option value="prompt">提示词 JSON · 兼容传统模型</option></select><p class="field-help">DeepSeek 官方接口自动使用 json_object；其他接口优先 JSON Schema，格式被明确拒绝时逐级兼容。Temperature 被明确拒绝时仅省略该参数重试一次。</p></div>
        <div class="connection-actions"><button class="button secondary" type="button" :disabled="!!working || !store.ready" @click="test"><span v-if="working === 'test'" class="spinner"></span><Icon v-else name="shield" :size="16" />{{ working === 'test' ? '正在测试…' : '测试连接' }}</button><span class="field-help">会发送一条短文本翻译请求，可能产生少量 API 费用。</span></div>
      </section>

      <section class="settings-card"><div class="settings-card-title"><Icon name="keyboard" /><div><h2>翻译偏好</h2><p>快捷键、语言和阅读体验</p></div></div>
        <div class="form-row"><div class="field"><label for="shortcut">全局快捷键</label><input id="shortcut" v-model="form.shortcut" placeholder="Ctrl+Shift+Q" :disabled="!!working" required /><p class="field-help">如 Ctrl+Shift+Q 或 Alt+Shift+T。</p></div><div class="field"><label for="capture-mode">文字获取方式</label><select id="capture-mode" v-model="form.captureMode" :disabled="!!working"><option value="auto">自动选区 · 按系统选择</option><option value="clipboard">仅剪贴板 · 先手动复制</option></select></div></div>
        <ShortcutSettings :draft-shortcut="form.shortcut" :working="!!working" />
        <div class="field-help capture-help">Windows：自动模式模拟 Ctrl+C，并尽力恢复原剪贴板文本；富文本格式无法完整恢复。X11：优先 PRIMARY 选区。Wayland：始终先 Ctrl+C 复制后翻译；可启用系统快捷键，或使用应用与托盘中的「翻译剪贴板」。</div>
        <div class="form-row"><div class="field"><label for="target-language">翻译为</label><select id="target-language" v-model="form.targetLanguage" :disabled="!!working"><option>简体中文</option><option>English</option><option>日本語</option></select></div><div class="field"><label for="theme">外观</label><select id="theme" v-model="form.theme" :disabled="!!working"><option value="system">跟随系统</option><option value="light">浅色</option><option value="dark">深色</option></select></div></div>
        <label class="toggle-row" for="explain-terms"><div><strong>专业术语解释</strong><p>理解 Java、JVM、AI、数据库等领域的术语；跳过普通词汇。</p></div><input id="explain-terms" v-model="form.explainTerms" type="checkbox" role="switch" :disabled="!!working" /></label>
      </section>
      <section class="settings-card"><div class="settings-card-title"><Icon name="book" /><div><h2>记忆与个性化</h2><p>功能默认关闭；所有更改保存后生效</p></div></div>
        <label class="toggle-row" for="remember-terms"><div><strong>记住专业术语</strong><p>每次成功翻译后，在本机保存术语、译名、领域、独立词义和出现时间、来源。即使关闭术语展示也会检测。不保存原文、完整译文或示例。</p></div><input id="remember-terms" v-model="form.rememberTerms" type="checkbox" role="switch" :disabled="!!working" /></label>
        <label class="toggle-row" for="personalize"><div><strong>个性化解释</strong><p>将相关本地术语和主题画像随翻译发送至模型，结合你明确设置的偏好和熟悉程度。</p></div><input id="personalize" v-model="form.personalizeExplanations" type="checkbox" role="switch" :disabled="!!working" /></label>
        <label class="toggle-row" for="auto-analyze"><div><strong>自动分析术语记忆</strong><p>仅在应用运行且空闲时检查。至少 20 条新记录，每 24 小时最多一次自动尝试。会向已配置的模型发送术语摘要和反馈，可能产生 API 费用。</p></div><input id="auto-analyze" v-model="form.autoAnalyze" type="checkbox" role="switch" :disabled="!!working || !form.rememberTerms" /></label>
        <div class="form-row memory-preferences"><div class="field"><label for="explanation-style">解释风格</label><select id="explanation-style" v-model="form.explanationStyle" :disabled="!!working"><option value="balanced">均衡 · 清晰与细节兼顾</option><option value="introductory">入门 · 从基础讲起</option><option value="concise">简洁 · 关注关键含义</option><option value="practical">实用 · 偏重编程示例</option></select></div><div class="field"><label for="background-domain">参考背景</label><select id="background-domain" v-model="form.backgroundDomain" :disabled="!!working"><option value="general">不指定背景</option><option value="backend">后端工程</option><option value="ai">AI / LLM</option><option value="database">数据库</option></select></div></div>
        <p class="field-help">出现频率只用于主题分析，不代表你已经掌握。已保存的术语可在「术语记忆」中搜索、反馈、删除和清空。更改记忆或自动分析开关会使现有画像失效，避免旧请求写回；已有术语保留。</p>
      </section>
      <div v-if="failure" class="inline-message error" role="alert">{{ failure }}</div><div v-if="message" class="inline-message success" role="status">{{ message }}</div>
      <UsageLine :snapshot="testUsage" />
      <p v-if="failure && !testUsage" class="field-help">此次操作未产生已发送的模型请求。</p>
      <div class="save-bar"><span><Icon name="shield" :size="15" />API Key 由系统安全凭据存储保护</span><button class="button primary" type="submit" :disabled="!!working || shortcuts.pending || !store.ready"><span v-if="working === 'save'" class="spinner"></span><Icon v-else name="check" :size="16" />{{ working === 'save' ? '正在保存…' : '保存设置' }}</button></div>
    </form>
    <UsageSettings />
    <SyncSettings />
  </section>
</template>
