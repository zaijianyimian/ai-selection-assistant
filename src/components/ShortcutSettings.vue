<script setup lang="ts">
import { computed, onMounted, watch } from 'vue'
import { useShortcuts, shortcutLabels } from '../stores/shortcuts'
import { useAssistant } from '../stores/assistant'

const props = defineProps<{ draftShortcut: string; working: boolean }>()
const store = useAssistant()
const shortcuts = useShortcuts()
const dirty = computed(() => props.draftShortcut.trim() !== store.settings.shortcut.trim())
const blocked = computed(() => props.working || shortcuts.pending || !store.ready)
onMounted(() => { void shortcuts.refresh() })
watch(() => store.settings.shortcut, () => { void shortcuts.refresh() })
</script>

<template>
  <div class="shortcut-settings" data-testid="shortcut-settings" :aria-busy="shortcuts.pending">
    <p role="status"><strong>快捷键状态：{{ shortcuts.state ? shortcutLabels[shortcuts.state.status] : '正在检测' }}</strong></p>
    <template v-if="shortcuts.state">
      <p class="field-help">{{ shortcuts.state.message }}</p>
      <p v-if="shortcuts.state.observed" class="field-help">系统当前组合键：<kbd>{{ shortcuts.state.observed }}</kbd></p>
      <template v-if="shortcuts.state.backend !== 'native'">
        <p class="field-help">Wayland：先 Ctrl+C 复制文字，再按系统快捷键。快捷键仅保存在本机。</p>
        <p class="shortcut-command"><code>{{ shortcuts.state.command }}</code></p>
        <p v-if="dirty" class="field-help">请先保存新的快捷键，再启用或重新应用。</p>
        <div class="shortcut-actions">
          <template v-if="shortcuts.state.backend === 'gnome'">
            <template v-if="shortcuts.state.status === 'recovery-required'">
              <button class="button secondary" type="button" :disabled="blocked" @click="shortcuts.manage('recover', store.settings.shortcut)">恢复快捷键操作</button>
              <p class="field-help">若提示为设置恢复，请重新输入或明确删除 API Key 后保存设置。</p>
            </template>
            <template v-else>
              <button class="button secondary" type="button" :disabled="blocked || dirty || !shortcuts.state.capable" @click="shortcuts.manage(shortcuts.state!.enabled ? 'reapply' : 'enable', store.settings.shortcut)">{{ shortcuts.state.enabled ? '重新应用已保存快捷键' : '启用系统快捷键' }}</button>
              <button class="button secondary" type="button" :disabled="blocked || !shortcuts.state.capable || (!shortcuts.state.enabled && !shortcuts.state.observed)" @click="shortcuts.manage('disable', store.settings.shortcut)">停用系统快捷键</button>
            </template>
          </template>
          <button class="button secondary" type="button" :disabled="blocked" @click="shortcuts.refresh()">刷新状态</button>
        </div>
        <p class="field-help">安装和升级不自动改动系统快捷键。启用后退出应用仍可用；升级后请退出旧版本再重新启动。系统中修改或删除的配置不会自动覆盖。</p>
      </template>
    </template>
    <p v-if="shortcuts.error" class="inline-message error" role="status">{{ shortcuts.error }}</p>
  </div>
</template>
