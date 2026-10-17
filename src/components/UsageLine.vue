<script setup lang="ts">
import { computed } from 'vue'
import type { UsageSnapshot } from '../types'
import { useUsage } from '../stores/usage'
import { coverage, metricText, tokens } from '../usage'
const props = defineProps<{ snapshot?: UsageSnapshot | null }>()
const store = useUsage()
const excluded = computed(() => props.snapshot?.excludedAfterReset || (store.view && props.snapshot && props.snapshot.generation < store.view.generation))
</script>

<template>
  <div v-if="snapshot" class="usage-line" aria-label="本次 Token 用量">
    <p v-if="snapshot.source === 'local-cache'">本地缓存 · 本次新增 0 Token · 未请求模型</p>
    <template v-else-if="snapshot.usage.attempts || snapshot.usage.pending">
      <p class="usage-numbers"><span>本次 Token</span><span>输入 {{ metricText(snapshot.usage.input) }}</span><span>输出 {{ metricText(snapshot.usage.output) }}</span><span>合计 {{ metricText(snapshot.usage.total) }}</span></p>
      <p>{{ coverage(snapshot.usage) }}</p>
      <p v-if="snapshot.usage.cachedInput.tokens !== null || snapshot.usage.reasoningOutput.tokens !== null">其中：服务端缓存输入 {{ tokens(snapshot.usage.cachedInput.tokens) }} · 推理输出 {{ tokens(snapshot.usage.reasoningOutput.tokens) }}（子集，不另加总）</p>
    </template>
    <p v-else>未发送模型请求 · 无新增用量</p>
    <p v-if="snapshot.persistenceIncomplete" class="danger">用量统计未完整保存，本次已观察数字仍可阅读。</p>
    <p v-if="excluded">该操作开始于统计清空之前，未纳入清空后的累计。</p>
  </div>
</template>
