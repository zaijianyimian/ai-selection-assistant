<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useAssistant } from './stores/assistant'
import { command, desktop } from './native'
import { errorMessage } from './types'
import Icon from './components/Icon.vue'
import { useShortcuts } from './stores/shortcuts'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import appIcon from './assets/app-icon.png'
import TranslationView from './components/TranslationView.vue'
import SettingsView from './components/SettingsView.vue'
import MemoryView from './components/MemoryView.vue'
import { useUsage } from './stores/usage'

const store = useAssistant()
const usage = useUsage()
onMounted(() => { void usage.initialize() })
onUnmounted(() => { usage.dispose() })
const shortcuts = useShortcuts()
let stopShortcuts: UnlistenFn | undefined
let disposed = false
const popup = new URLSearchParams(window.location.search).get('window') === 'translation'
const colorScheme = window.matchMedia('(prefers-color-scheme: dark)')
function updateTheme() {
  document.documentElement.dataset.theme = store.settings.theme === 'system' ? (colorScheme.matches ? 'dark' : 'light') : store.settings.theme
}
watch(() => store.settings.theme, updateTheme, { immediate: true })
colorScheme.addEventListener('change', updateTheme)
async function action(name: string) { try { await command(name) } catch (e) { store.error = errorMessage(e) } }
async function hide() { if (desktop) await getCurrentWindow().hide().catch(e => { store.error = errorMessage(e) }) }
function keydown(event: KeyboardEvent) { if (popup && event.key === 'Escape') void hide() }
onMounted(() => { void store.initialize(); if (desktop) void listen('shortcuts-changed', () => { void shortcuts.refresh() }).then(stop => { if (disposed) stop(); else stopShortcuts = stop }).catch(e => { shortcuts.error = errorMessage(e) }); window.addEventListener('keydown', keydown) })
onUnmounted(() => { disposed = true; stopShortcuts?.(); store.dispose(); colorScheme.removeEventListener('change', updateTheme); window.removeEventListener('keydown', keydown) })
</script>

<template>
  <div class="app-shell" :class="{ 'popup-shell': popup }">
    <aside v-if="!popup" class="sidebar"><a class="brand" href="#" @click.prevent="store.page = 'translate'"><span class="brand-mark"><img :src="appIcon" alt="" /></span><span>Selection<span class="brand-subtitle">AI ASSISTANT</span></span></a><div class="workspace-label">工作空间</div><nav aria-label="主要导航"><button :class="['nav-item', { active: store.page === 'translate' }]" @click="store.page = 'translate'"><Icon name="translate" />划词翻译<span class="nav-indicator"></span></button><button :class="['nav-item', { active: store.page === 'memory' }]" @click="store.page = 'memory'"><Icon name="book" />术语记忆</button><button :class="['nav-item', { active: store.page === 'settings' }]" @click="store.page = 'settings'"><Icon name="settings" />偏好设置</button></nav><div class="sidebar-bottom"><div class="sidebar-note"><span class="tiny-star">✧</span><strong>知识，不止于翻译</strong><p>从一句话，到一个概念。<br />让每次阅读多一点收获。</p></div><div class="local-label"><Icon name="shield" :size="15" /><span>无需登录 · 自带模型</span></div><button class="quit-button" @click="action('quit_app')"><Icon name="power" :size="14" />退出应用</button><span class="version">开源桌面客户端 · v0.4.0</span></div></aside>
    <div class="main-shell"><header class="topbar"><template v-if="popup"><div class="popup-brand"><span class="brand-mark small"><img :src="appIcon" alt="" /></span><strong>划词翻译</strong></div><div class="topbar-actions"><button class="icon-button" aria-label="打开设置" title="打开设置" @click="action('show_settings')"><Icon name="settings" :size="18" /></button><button class="icon-button" aria-label="隐藏翻译窗口" title="隐藏 · Esc" @click="hide"><Icon name="close" :size="19" /></button></div></template><template v-else><span class="breadcrumb">工作空间 <span>/</span> <strong>{{ store.page === 'translate' ? '划词翻译' : store.page === 'memory' ? '术语记忆' : '偏好设置' }}</strong></span><div class="topbar-actions"><span class="model-badge"><span class="status-dot"></span>{{ store.settings.model }}</span><button class="icon-button" title="打开悬浮窗" aria-label="打开悬浮翻译窗口" @click="action('show_popup')"><Icon name="popup" :size="17" /></button></div></template></header>
      <main><div v-if="!desktop" class="inline-message warning">当前为浏览器界面预览。运行 <code>npm run tauri dev</code> 使用原生翻译、快捷键和系统凭据存储。</div><div v-for="(warning, index) in store.warnings" :key="index" class="inline-message warning"><Icon name="info" :size="16" />{{ warning }}</div><div v-if="store.error" class="inline-message error" role="alert"><span>{{ store.error }}</span><button class="icon-button" aria-label="关闭错误提示" @click="store.error = ''"><Icon name="close" :size="15" /></button></div><SettingsView v-if="!popup && store.page === 'settings'" /><MemoryView v-else-if="!popup && store.page === 'memory'" /><TranslationView v-else :popup="popup" /></main>
      <footer class="app-footer"><span><span class="status-dot"></span>{{ store.platform }}</span><span v-if="!popup">为专注阅读而设计</span></footer>
    </div>
  </div>
</template>
