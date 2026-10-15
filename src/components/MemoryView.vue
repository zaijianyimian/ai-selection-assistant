<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useMemory } from '../stores/memory'
import { useAssistant } from '../stores/assistant'
import type { Familiarity } from '../types'
import Icon from './Icon.vue'
const memory = useMemory()
const assistant = useAssistant()
const deleteId = ref<number | null | undefined>(undefined)
const active = computed(() => memory.view.versions.find(v => v.id === memory.view.activeVersion))
const date = (at: number) => at ? new Date(at * 1000).toLocaleString() : '尚未运行'
onMounted(() => { void memory.initialize() })
onUnmounted(() => memory.dispose())
async function search() { memory.offset = 0; await memory.refresh() }
async function page(direction: number) { memory.offset = Math.max(0, memory.offset + direction * 30); await memory.refresh() }
async function remove() { const id = deleteId.value; deleteId.value = undefined; if (id !== undefined) await memory.act('delete_memory', { id }) }
</script>

<template>
  <section class="memory-view">
    <div class="page-heading"><div><div class="eyebrow">KNOWLEDGE THAT STAYS</div><h1>术语记忆<span>。</span></h1><p>让解释贴近你的阅读习惯，熟悉程度由你决定。</p></div></div>
    <div class="inline-message" :class="assistant.settings.rememberTerms ? 'success' : 'warning'">{{ assistant.settings.rememberTerms ? '记忆已开启：仅保存专业术语、领域、独立词义和出现记录。' : '记忆已暂停。可在偏好设置中开启，已保存数据仍可查看和删除。' }}不保存原文、完整译文和示例。</div>
    <section class="settings-card memory-analysis">
      <div class="settings-card-title"><Icon name="spark" /><div><h2>阅读主题分析</h2><p>将术语摘要与反馈发送给已配置的模型，可能产生 API 费用。</p></div></div>
      <div class="memory-actions"><button class="button primary" :disabled="!assistant.ready || !assistant.settings.rememberTerms || memory.working || memory.status.running || !memory.view.pending" @click="memory.act('analyze_memory')"><span v-if="memory.status.running" class="spinner"></span>{{ memory.status.running ? '正在分析…' : '分析新增记忆' }}</button><span class="muted">{{ memory.view.pending }} 条记录待分析 · 每批最多 100 个术语</span></div>
      <p class="field-help">{{ assistant.settings.autoAnalyze ? '自动分析已开启：应用运行且空闲时检查，至少 20 条新记录，每 24 小时至多一次自动尝试。' : '自动分析已关闭。可在偏好设置中开启。' }}上次自动尝试：{{ date(memory.view.lastAutoAttempt) }}</p>
      <p v-if="memory.status.message" role="status">{{ memory.status.message }}</p><div v-if="memory.status.error" class="inline-message error" role="alert">{{ memory.status.error.message }}</div>
      <template v-if="memory.view.versions.length"><label for="profile-version">分析版本</label><select id="profile-version" :value="memory.view.activeVersion" :disabled="memory.working" @change="memory.act('select_profile', { id: Number(($event.target as HTMLSelectElement).value) })"><option v-for="version in memory.view.versions" :key="version.id" :value="version.id">#{{ version.id }} · {{ date(version.createdAt) }}</option></select><div class="profile-topics"><span v-for="topic in active?.profile.topics" :key="topic.domain" class="subtle-badge">{{ topic.domain }} · 证据 #{{ topic.evidenceIds.join('、#') }}</span></div><p class="field-help">主题表示近期兴趣，不表示专业水平。删除数据或更改记忆开关会使这些画像失效。</p></template>
      <p v-else class="muted">尚无分析画像。你仍可使用明确设置的背景与解释风格。</p>
    </section>
    <div class="section-heading"><div><Icon name="book" /><h2>本地术语</h2><span class="count-badge">{{ memory.view.total }}</span></div><button class="text-button danger" :disabled="memory.working || !memory.view.total" @click="deleteId = null">清空全部记忆</button></div>
    <form class="memory-search" @submit.prevent="search"><label for="term-search" class="sr-only">搜索术语</label><input id="term-search" v-model="memory.query" placeholder="搜索名称、译名或领域" maxlength="200" /><button class="button secondary" type="submit">搜索</button><button class="text-button" type="button" @click="memory.refresh">刷新</button></form>
    <div v-if="memory.error" class="inline-message error" role="alert">{{ memory.error }}</div>
    <div v-if="deleteId !== undefined" class="inline-message warning" role="alert"><span>{{ deleteId === null ? '确认清空所有术语和反馈？' : '确认删除此术语及其出现记录？' }}相关分析画像也将清除，无法撤销。</span><button class="button secondary" @click="deleteId = undefined">取消</button><button class="button primary" @click="remove">确认删除</button></div>
    <div class="term-grid"><article v-for="term in memory.view.terms" :key="term.id" class="term-card"><div class="memory-term-heading"><h3>{{ term.original }} <small class="muted">#{{ term.id }}</small></h3><span class="domain-tag">{{ term.domain }}</span></div><strong>{{ term.translation }}</strong><p>{{ term.senseSummary }}</p><p class="field-help">{{ term.count }} 次出现 · {{ term.language }} · {{ date(term.lastSeen) }}</p><div class="memory-actions"><label :for="`familiarity-${term.id}`">熟悉程度</label><select :id="`familiarity-${term.id}`" :value="term.familiarity" :disabled="memory.working" @change="memory.feedback(term.id, ($event.target as HTMLSelectElement).value as Familiarity)"><option value="unknown">未标记</option><option value="learning">正在学习</option><option value="known">已熟悉</option></select><button class="text-button danger" :disabled="memory.working" @click="deleteId = term.id">删除</button></div></article></div>
    <p v-if="!memory.view.terms.length" class="terms-empty">暂无匹配术语。开启记忆后，完成一次含专业术语的翻译即可积累。</p>
    <div class="memory-actions pagination"><button class="button secondary" :disabled="memory.offset === 0" @click="page(-1)">上一页</button><span>{{ Math.floor(memory.offset / 30) + 1 }} / {{ Math.max(1, Math.ceil(memory.view.total / 30)) }}</span><button class="button secondary" :disabled="memory.offset + 30 >= memory.view.total" @click="page(1)">下一页</button></div>
  </section>
</template>
