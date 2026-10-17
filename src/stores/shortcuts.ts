import { defineStore } from 'pinia'
import { ref } from 'vue'
import { command, desktop } from '../native'
import { errorMessage } from '../types'

export interface ShortcutStatus {
  backend: 'native' | 'gnome' | 'manual'
  capable: boolean
  enabled: boolean
  desired: string
  observed: string | null
  status: 'ready' | 'configured' | 'not-configured' | 'external-change' | 'conflict' | 'unavailable' | 'recovery-required'
  message: string
  command: string
}
export const shortcutLabels: Record<ShortcutStatus['status'], string> = {
  ready: '已注册', configured: '系统已配置 · 待按键验证', 'not-configured': '未配置',
  'external-change': '系统配置已变更', conflict: '快捷键冲突', unavailable: '不可用 · 可手动配置', 'recovery-required': '需要恢复',
}
export const useShortcuts = defineStore('shortcuts', () => {
  const state = ref<ShortcutStatus | null>(null)
  const pending = ref(false)
  const error = ref('')
  let revision = 0
  function accept(value: ShortcutStatus | null | undefined) {
    if (value && ['native', 'gnome', 'manual'].includes(value.backend)) state.value = value
  }
  async function refresh() {
    if (!desktop || pending.value) return
    const current = ++revision
    try { const value = await command<ShortcutStatus>('get_shortcut_status'); if (current === revision) accept(value) }
    catch (e) { if (current === revision) error.value = errorMessage(e) }
  }
  async function manage(action: 'enable' | 'reapply' | 'disable' | 'recover', expectedShortcut: string) {
    if (pending.value) return
    pending.value = true; error.value = ''; ++revision
    try { accept(await command<ShortcutStatus>('manage_shortcut', { action, expectedShortcut })) }
    catch (e) { error.value = errorMessage(e) }
    finally { pending.value = false }
    await refresh()
  }
  return { state, pending, error, refresh, manage }
})
