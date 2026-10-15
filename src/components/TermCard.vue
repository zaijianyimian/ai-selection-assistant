<script setup lang="ts">
import { ref, watch } from 'vue'
import { command } from '../native'
import { errorMessage, type Familiarity, type Term } from '../types'
const props = defineProps<{ term: Term }>()
const feedback = ref<Familiarity>('unknown')
const message = ref('')
const working = ref(false)
watch(() => props.term, () => { feedback.value = 'unknown'; message.value = '' })
async function rate(value: Familiarity) {
  working.value = true; message.value = ''
  try { await command('set_term_feedback', { id: props.term.memoryId, value }); feedback.value = value; message.value = '已保存反馈' }
  catch (e) { message.value = errorMessage(e) }
  finally { working.value = false }
}
</script>

<template>
  <article class="term-card">
    <header><div><strong>{{ term.original }}</strong><span class="term-translation">{{ term.translation }}</span></div><span class="domain-tag">{{ term.domain }}</span></header>
    <p>{{ term.explanation }}</p>
    <div v-if="term.example" class="term-example"><span>例如</span>{{ term.example }}</div>
    <div v-if="term.memoryId" class="term-feedback"><span class="muted">记忆 #{{ term.memoryId }}</span><button class="text-button" :aria-pressed="feedback === 'learning'" :disabled="working" @click="rate('learning')">正在学习</button><button class="text-button" :aria-pressed="feedback === 'known'" :disabled="working" @click="rate('known')">已熟悉</button><button v-if="feedback !== 'unknown'" class="text-button" :disabled="working" @click="rate('unknown')">取消标记</button></div><p v-if="message" class="field-help" role="status">{{ message }}</p>
  </article>
</template>
