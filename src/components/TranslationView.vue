<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { useAssistant } from '../stores/assistant'
import { command } from '../native'
import { errorMessage } from '../types'
import Icon from './Icon.vue'
import TermCard from './TermCard.vue'
import { useShortcuts, shortcutLabels } from '../stores/shortcuts'

defineProps<{ popup: boolean }>()
const store = useAssistant()
const shortcuts = useShortcuts()
onMounted(() => { void shortcuts.refresh() })
const wayland = computed(() => store.platform.startsWith('Wayland') || ['gnome', 'manual'].includes(shortcuts.state?.backend ?? ''))
const copied = ref(false)
const copyError = ref('')
const targetLanguage = computed(() => store.job.requestOptions?.targetLanguage ?? (store.job.result ? '目标语言未知' : store.settings.targetLanguage))
const explainTerms = computed(() => store.job.requestOptions?.explainTerms ?? (store.job.result ? !!store.job.result.terms.length : store.settings.explainTerms))
const count = computed(() => [...store.input].length)
const sourceLabels: Record<string, string> = { manual: '手动输入', 'windows-selection': 'Windows 选区', 'x11-primary': 'X11 选区', 'wayland-clipboard': 'Wayland 剪贴板', clipboard: '剪贴板' }
async function copy() {
  copyError.value = ''
  try {
    await command('copy_translation')
    copied.value = true
    setTimeout(() => { copied.value = false }, 1800)
  } catch (e) { copyError.value = errorMessage(e) }
}
</script>

<template>
  <section class="translation-view" :class="{ compact: popup }">
    <div v-if="!popup" class="page-heading"><div><div class="eyebrow">YOUR WORDS, UNDERSTOOD</div><h1>读懂每一段文字<span>。</span></h1><p>即刻翻译，理解藏在文字里的专业知识。</p></div><span class="subtle-badge"><span class="status-dot"></span> 本地客户端</span></div>
    <div v-if="!popup" class="shortcut-strip"><span class="shortcut-icon"><Icon name="keyboard" /></span><div><strong>{{ wayland ? '先复制文字，再唤起翻译' : '选中文字，唤起翻译' }}</strong><p v-if="wayland">先 Ctrl+C；{{ shortcuts.state ? shortcutLabels[shortcuts.state.status] : '请在设置中检查系统快捷键' }}。{{ shortcuts.state?.message }}</p><p v-else>在浏览器、编辑器或阅读器中，按下已注册的全局快捷键。</p></div><div class="key-combo"><kbd v-for="key in (shortcuts.state?.observed || store.settings.shortcut).split('+')" :key="key">{{ key }}</kbd></div></div>

    <p v-if="popup && store.platform.startsWith('Wayland')" class="field-help placement-hint">Wayland 不支持自动贴近选区；可拖动悬浮窗，本次运行隐藏后再打开会保留手动位置。</p>
    <div class="translation-grid">
      <section class="text-panel input-panel">
        <div class="panel-heading"><span><span class="mini-label">01</span>原文</span><span class="muted text-small">{{ store.job.result?.sourceLanguage || '自动检测语言' }}</span></div>
        <label for="source-text" class="sr-only">输入要翻译的文字</label>
        <textarea id="source-text" v-model="store.input" placeholder="粘贴或输入想要理解的文字…&#10;&#10;Wayland 请先 Ctrl+C 复制，再使用系统快捷键翻译。" :disabled="store.busy" @keydown.ctrl.enter.prevent="store.translate()" />
        <div class="input-meta"><span :class="{ danger: count > 16000 }">{{ count.toLocaleString() }} / 16,000</span><button class="text-button" :disabled="store.busy || !store.input" @click="store.input = ''">清空</button></div>
        <div class="input-actions"><button class="button secondary" :disabled="store.busy || !store.ready" @click="store.translate(true)"><Icon name="clipboard" :size="16" />翻译剪贴板</button><button class="button primary" :disabled="store.busy || !store.ready || !store.input.trim() || count > 16000" @click="store.translate()"><span v-if="store.busy" class="spinner"></span><Icon v-else name="spark" :size="16" />{{ store.busy ? '处理中' : '开始翻译' }}</button></div>
      </section>

      <section class="text-panel output-panel" aria-live="polite" aria-atomic="false">
        <div class="panel-heading"><span><span class="mini-label accent">02</span>译文</span><div class="translation-tools"><span class="language-tag">{{ targetLanguage }}</span><button class="icon-button" :aria-label="copied ? '已复制' : '复制译文'" :title="copied ? '已复制' : '复制译文'" :disabled="!store.job.result || store.busy" @click="copy"><Icon :name="copied ? 'check' : 'copy'" :size="16" /></button></div></div>
        <div v-if="store.busy" class="result-loading" role="status"><div class="loading-symbol"><Icon name="spark" :size="26" /></div><strong>{{ store.job.status === 'capturing' ? '正在读取文字…' : '正在理解这段文字…' }}</strong><p>翻译与术语解释将在同一次回复中返回</p><div class="skeleton-lines"><i></i><i></i><i></i></div></div>
        <template v-else-if="store.job.result"><div class="translated-text">{{ store.job.result.translation }}</div><div class="result-meta"><span class="status-dot"></span>翻译完成<span v-if="store.job.source"> · {{ sourceLabels[store.job.source] || store.job.source }}</span></div></template>
        <div v-else-if="store.job.error" class="result-error" role="alert"><Icon name="info" :size="28" /><strong>这次翻译没有完成</strong><p>{{ store.job.error.message }}</p><button class="button secondary" :disabled="!store.input.trim()" @click="store.translate()">重试输入文字</button></div>
        <div v-else class="empty-result"><div class="empty-icon"><Icon name="translate" :size="32" /></div><strong>让理解，自然发生</strong><p>译文会出现在这里<br />专业术语也会得到清晰的解释</p><span class="empty-line"></span><span class="muted text-small">使用你自己的模型，连接更广阔的知识</span></div>
        <div v-if="copyError" class="inline-message error" role="alert">{{ copyError }}</div>
      </section>
    </div>

    <div v-if="store.job.warning" class="inline-message warning"><Icon name="info" :size="16" />{{ store.job.warning }}</div>
    <div v-if="store.job.result?.compatibilityFallback" class="inline-message warning">已按服务返回的兼容要求调整输出格式或 Temperature 参数，翻译成功。可在设置中选择「JSON 模式」或「提示词 JSON」减少后续格式探测。</div>

    <section class="terms-section"><div class="section-heading"><div><Icon name="book" :size="18" /><h2>术语笔记</h2><span v-if="store.job.result" class="count-badge">{{ store.job.result.terms.length }}</span></div><span class="muted text-small">{{ explainTerms ? '在语境中，理解专业词汇' : '术语解释已关闭' }}</span></div>
      <div v-if="store.job.result?.terms.length" class="term-grid"><TermCard v-for="(term, index) in store.job.result.terms" :key="`${term.original}-${index}`" :term="term" /></div>
      <div v-else class="terms-empty"><Icon name="spark" :size="19" /><span>{{ store.job.result ? '这段文字中没有需要解释的专业术语。' : '翻译后，值得了解的专业术语会收录在这里。' }}</span><span v-if="!popup" class="domain-hint">Java · AI / LLM · 数据库 · 软件工程</span></div>
    </section>
    <p class="privacy-note"><Icon name="shield" :size="14" />原文与完整译文不落盘。{{ store.settings.rememberTerms ? '术语记忆已开启，仅保存精简术语与出现记录。' : '术语记忆已关闭。' }}{{ store.settings.autoAnalyze ? '自动分析会将术语摘要发送至已配置的模型。' : '自动分析已关闭。' }}</p>
  </section>
</template>
