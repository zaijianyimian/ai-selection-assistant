<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useUsage } from '../stores/usage'
import { coverage, metricText, purposeLabels } from '../usage'
import Icon from './Icon.vue'
const store = useUsage()
const confirm = ref(false)
const model = ref('')
const service = ref('')
const services = computed(() => [...new Map(store.view?.groups.map(group => [group.service, group.label]) ?? []).entries()])
const periods = computed(() => store.view ? [{ title: '今日', data: store.view.today }, { title: '本月', data: store.view.month }, { title: '累计', data: store.view.cumulative }] : [])
onMounted(() => { void store.refresh() })
async function filter() { store.query.offset = 0; store.query.model = model.value.trim() || null; store.query.service = service.value.trim() || null; await store.refresh() }
async function page(step: number) { store.query.offset = Math.max(0, store.query.offset + step * 20); await store.refresh() }
async function clear() { if (await store.clear()) confirm.value = false }
</script>

<template>
  <section class="settings-card usage-settings" aria-labelledby="usage-heading">
    <div class="settings-card-title"><Icon name="spark" /><div><h2 id="usage-heading">Token 用量</h2><p>本应用在本机观测到的服务返回值，重启后保留。</p></div></div>
    <div v-if="store.error" class="inline-message error" role="alert">{{ store.error }}历史数据可能未完整保存，请检查后重试读取。</div>
    <div v-if="store.view?.incomplete" class="inline-message warning" role="status">累计统计存在保存缺口；以下数字仅为已保存的报告值。</div>
    <template v-if="store.view">
      <p class="field-help">起算：{{ new Date(store.view.since * 1000).toLocaleString() }} · 当前本机日期 {{ store.view.date }}。不补算启用前的历史。</p>
      <div class="usage-periods"><article v-for="period in periods" :key="period.title"><h3>{{ period.title }}</h3><dl><div><dt>输入</dt><dd>{{ metricText(period.data.input) }}</dd></div><div><dt>输出</dt><dd>{{ metricText(period.data.output) }}</dd></div><div><dt>合计 Token</dt><dd>{{ metricText(period.data.total) }}</dd></div></dl><p class="field-help">{{ coverage(period.data) }}</p></article></div>
      <form class="usage-filters" @submit.prevent="filter"><div class="field"><label for="usage-purpose">用途</label><select id="usage-purpose" v-model="store.query.purpose" :disabled="store.pending" @change="filter"><option :value="null">全部用途</option><option v-for="(label, key) in purposeLabels" :key="key" :value="key">{{ label }}</option></select></div><div class="field"><label for="usage-model">模型（精确匹配）</label><input id="usage-model" v-model="model" maxlength="200" /></div><div class="field"><label for="usage-service">服务</label><select id="usage-service" v-model="service" :disabled="store.pending" @change="filter"><option value="">全部服务</option><option v-for="([id, label], index) in services" :key="id" :value="id">{{ label }} · 服务 {{ index + 1 }}</option></select></div><button class="button secondary" type="submit" :disabled="store.pending">筛选</button></form>
      <p class="field-help">筛选影响以上三个统计区间与下方累计分类。更换同服务/模型的 API Key 不会分开计算账户。</p>
      <ul v-if="store.view.groups.length" class="usage-groups"><li v-for="group in store.view.groups" :key="`${group.service}/${group.model}/${group.purpose}`"><div><strong>{{ group.label }} · {{ group.model }}</strong><span>{{ purposeLabels[group.purpose] }}</span><button class="text-button" type="button" @click="service = group.service; filter()">仅此服务</button></div><p>输入 {{ metricText(group.usage.input) }} · 输出 {{ metricText(group.usage.output) }} · 合计 {{ metricText(group.usage.total) }}</p><p class="field-help">{{ coverage(group.usage) }}</p></li></ul>
      <p v-else class="field-help">暂无匹配的用量记录。</p>
      <div v-if="store.view.groupCount > 20" class="memory-actions"><button class="button secondary" :disabled="store.pending || !store.query.offset" @click="page(-1)">上一页</button><span>{{ Math.floor(store.query.offset / 20) + 1 }} / {{ Math.ceil(store.view.groupCount / 20) }}</span><button class="button secondary" :disabled="store.pending || store.query.offset + 20 >= store.view.groupCount" @click="page(1)">下一页</button></div>
    </template>
    <div class="memory-actions"><button class="button secondary" :disabled="store.pending" @click="store.refresh">{{ store.pending ? '读取中…' : '刷新用量' }}</button><button class="text-button danger" :disabled="store.pending || !store.view" @click="confirm = true">清空用量统计</button></div>
    <div v-if="confirm" class="inline-message warning usage-confirm" role="alert"><p>仅清空本机用量统计并重新起算。译文、缓存、术语、同步和模型配置保留。不会取消已发送的模型请求；清空前开始的操作不进入新的累计。</p><button class="button secondary" :disabled="store.pending" @click="confirm = false">取消清空</button><button class="button primary" :disabled="store.pending" @click="clear">确认清空用量</button></div>
    <details class="usage-help"><summary>Token、额度与统计范围说明</summary><p>Token 是模型处理文本的单位，不等于字数或字符数。输入还包括固定提示词、启用后的个性化摘要；输出包括结构化解释，以及模型报告的推理明细。单次输出上限和上下文长度限制属于模型请求能力，并不是账户剩余额度。</p><p>翻译、连接测试、手动及自动主题分析都会请求模型。兼容降级可能发送多次请求；失败、截断或拒绝也可能消耗 Token。服务未报告的字段显示“未知”，不按字符数估算；“0”仅代表明确报告的零或本地缓存零新增。</p><p>本地翻译缓存命中不请求模型，显示本次新增 0 Token；服务端上下文缓存仍是模型请求，其缓存输入和推理输出是父计数的子集，不再叠加。服务商仍可能对服务端缓存计费。</p><p>统计仅覆盖本应用在本机观察到的数据，未报告的请求和其他应用、设备用量不在其中。这不是账户配额、剩余余额或服务商账单，不计算金额或精确节省量；请在服务商控制台核对账单。</p><p>今日/本月按每次实际 HTTP 尝试开始时的本机日期归属。跨午夜或后来修改时区不移动历史记录。仅保存数字与服务/模型/用途分类，不保存原文、完整译文、提示词和 Key，不同步到其他设备。已结束尝试收据保留 7 天，日汇总保留到你主动清空；查看说明和统计无需联网。</p></details>
  </section>
</template>
