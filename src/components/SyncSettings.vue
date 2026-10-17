<script setup lang="ts">
import { onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { useSync } from '../stores/sync'
import { errorMessage } from '../types'
const sync = useSync()
const form = reactive({ url: '', password: '', deviceName: '我的电脑', consent: false, replaceTarget: false, importLocal: false })
const message = ref(''), failure = ref(''), external = ref(false), revoke = ref('')
let baseline = ''
watch(() => sync.view.url, url => { if (!form.url || form.url === baseline) { form.url = url; baseline = url } else if (url !== baseline) external.value = true })
onMounted(async () => { try { await sync.initialize(); if (sync.view.deviceName) form.deviceName = sync.view.deviceName } catch (e) { failure.value = errorMessage(e) } })
onUnmounted(() => { form.password = ''; sync.dispose() })
const statuses: Record<string, string> = { disabled: '已断开', idle: '已连接', syncing: '正在同步', offline: '离线，等待重试', 'needs-auth': '需要重新登录', conflict: '有冲突待处理', paused: '术语同步暂停', error: '同步出错' }
async function run(name: string, args?: Record<string, unknown>) { failure.value = ''; message.value = ''; try { return await sync.act(name, args) } catch (e) { failure.value = errorMessage(e) } }
async function connect(test: boolean) { try { const result = await run(test ? 'test_sync_connection' : 'connect_sync', { ...form, password: form.password, deviceName: form.deviceName }); if (typeof result === 'string') message.value = result; else if (result) message.value = '设备已连接。点击「立即同步」开始首轮同步；后台同步默认关闭。' } finally { form.password = '' } }
async function list() { try { await sync.listDevices() } catch (e) { failure.value = errorMessage(e) } }
async function revokeDevice() { const id = revoke.value; revoke.value = ''; await run('revoke_sync_device', { id }); if (sync.view.enabled) await list() }
function reload() { form.url = sync.view.url; baseline = form.url; external.value = false }
</script>
<template>
  <section class="settings-card sync-settings" data-testid="sync-settings">
    <div class="settings-card-title"><div><h2>多端同步</h2><p>连接你自己的 VPS，同步术语、熟悉程度与三项阅读偏好。</p></div></div>
    <div v-if="external" class="inline-message warning">同步地址已在其他窗口改变，当前编辑已保留。<button type="button" class="text-button" @click="reload">载入已保存地址</button></div>
    <div class="field"><label for="sync-url">同步服务地址</label><input id="sync-url" v-model="form.url" type="url" placeholder="https://sync.example.com" :disabled="sync.working" autocomplete="off" /><p class="field-help">需要 HTTPS。只有本机测试可使用 HTTP；填写同步服务地址。</p></div>
    <div class="form-row"><div class="field"><label for="sync-password">应用同步密码</label><input id="sync-password" v-model="form.password" type="password" autocomplete="new-password" :disabled="sync.working" /><p class="field-help">这是部署时设置的专用密码。测试或登录后清空，仅设备令牌保存在独立系统凭据项。</p></div><div class="field"><label for="sync-device-name">设备名称</label><input id="sync-device-name" v-model="form.deviceName" maxlength="80" :disabled="sync.working" /></div></div>
    <label class="checkbox-label"><input v-model="form.consent" type="checkbox" :disabled="sync.working" />我同意同步术语、译名、领域、词义、出现时间及来源、熟悉程度，以及外观、目标语言、解释风格。服务器可读取这些精简数据。</label>
    <p class="field-help">不上传原文、完整译文、示例、模型地址、API Key、画像和本机授权。关闭「记住专业术语」会暂停术语上传与导入，偏好仍可同步。备份可能保留历史数据，需在服务器单独管理。</p>
    <label class="checkbox-label"><input v-model="form.replaceTarget" type="checkbox" :disabled="sync.working" />确认更换服务／工作区／恢复后的历史，建立新同步基线</label>
    <label v-if="form.replaceTarget" class="checkbox-label"><input v-model="form.importLocal" type="checkbox" :disabled="sync.working" />将本机术语导入新的工作区（不勾选则清除本机旧术语后接收远端，偏好和模型配置保留）</label>
    <div class="memory-actions"><button type="button" class="button secondary" :disabled="sync.working || !form.url || !form.password" @click="connect(true)">测试同步连接</button><button type="button" class="button primary" :disabled="sync.working || !form.consent || !form.url || !form.password" @click="connect(false)">登录并连接</button></div>
    <div class="sync-status" aria-live="polite"><strong>{{ statuses[sync.view.status] || '同步状态未知' }}</strong><p>待提交 {{ sync.view.pending }} · 冲突 {{ sync.view.conflicts.length }} · 上次成功：{{ sync.view.lastSuccess ? new Date(sync.view.lastSuccess * 1000).toLocaleString() : '尚未同步' }}</p><p v-if="sync.view.message">{{ sync.view.message }}</p><p v-if="sync.view.discarded" class="inline-message warning">远端删除或清空已使 {{ sync.view.discarded }} 条旧待提交记录作废，未重新上传。</p><p v-if="sync.view.enabled && sync.view.memoryPaused">术语同步已暂停，阅读偏好仍可同步。</p></div>
    <template v-if="sync.view.deviceId">
      <label class="toggle-row"><div><strong>启用同步</strong><p>断开期间可操作本机记忆；重新连接后远端数据可能重新出现。</p></div><input type="checkbox" role="switch" :checked="sync.view.enabled" :disabled="sync.working" @change="run('configure_sync', { enabled: ($event.target as HTMLInputElement).checked, background: sync.view.background })" /></label>
      <label class="toggle-row"><div><strong>后台同步</strong><p>仅在应用运行时，每 60 秒检查；本机变更等待 2 秒后同步。退出后不会唤醒。</p></div><input type="checkbox" role="switch" :checked="sync.view.background" :disabled="sync.working || !sync.view.enabled" @change="run('configure_sync', { enabled: sync.view.enabled, background: ($event.target as HTMLInputElement).checked })" /></label>
      <div class="memory-actions"><button type="button" class="button primary" :disabled="sync.working || !sync.view.enabled || sync.view.status === 'needs-auth'" @click="run('sync_now')">{{ sync.working ? '正在处理…' : '立即同步' }}</button><button type="button" class="button secondary" :disabled="sync.working || !sync.view.enabled" @click="list">查看设备</button></div>
    </template>
    <article v-for="conflict in sync.view.conflicts" :key="conflict.id" class="sync-conflict"><strong>{{ conflict.field }}</strong><p>本机：{{ conflict.local }} · 远端：{{ conflict.remote ?? '已删除或清空' }}</p><div class="memory-actions"><button type="button" class="button secondary" :disabled="sync.working || conflict.code !== 'field_changed'" @click="run('resolve_sync_conflict', { id: conflict.id, keepLocal: true })">保留本机并重试</button><button type="button" class="button secondary" :disabled="sync.working" @click="run('resolve_sync_conflict', { id: conflict.id, keepLocal: false })">采用远端</button></div></article>
    <div v-for="device in sync.devices" :key="device.id" class="sync-device"><span>{{ device.name }} {{ device.id === sync.view.deviceId ? '（本机）' : '' }}</span><button type="button" class="text-button danger" :disabled="sync.working" @click="revoke = device.id">撤销设备</button></div>
    <div v-if="revoke" class="inline-message warning">确认撤销此设备？它必须重新登录才能同步。<button type="button" class="button secondary" @click="revoke = ''">取消</button><button type="button" class="button primary" @click="revokeDevice">确认撤销</button></div>
    <p v-if="message" class="inline-message success" role="status">{{ message }}</p><p v-if="failure || sync.error" class="inline-message error" role="alert">{{ failure || sync.error }}</p>
  </section>
</template>
<style scoped>
.sync-settings { margin-top: 24px; }
.sync-status,.sync-conflict { margin: 20px 0; padding: 16px; border: 1px solid var(--border); border-radius: 12px; overflow-wrap: anywhere; }
.sync-device { display: flex; align-items: center; justify-content: space-between; padding: 8px 0; }
</style>
