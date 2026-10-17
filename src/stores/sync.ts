import { ref } from 'vue'
import { defineStore } from 'pinia'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { command, desktop } from '../native'
import { errorMessage } from '../types'
export interface SyncConflict { id: string; field: string; local: string; remote: string | null; revision: number; code: string }
export interface SyncView { url: string; deviceName: string; deviceId: string; enabled: boolean; background: boolean; status: string; message: string; pending: number; conflicts: SyncConflict[]; lastSuccess: number; discarded: number; memoryPaused: boolean }
export interface SyncDevice { id: string; name: string; createdAt: number; lastSeen: number }
export const emptySync = (): SyncView => ({ url: '', deviceName: '', deviceId: '', enabled: false, background: false, status: 'disabled', message: '', pending: 0, conflicts: [], lastSuccess: 0, discarded: 0, memoryPaused: true })
export const useSync = defineStore('sync', () => {
  const view = ref(emptySync()), devices = ref<SyncDevice[]>([]), working = ref(false), error = ref('')
  let listener: UnlistenFn | undefined, revision = 0
  async function refresh() { if (!desktop) return; const n = ++revision; const v = await command<SyncView>('get_sync'); if (n === revision && v && typeof v.enabled === 'boolean') view.value = v }
  async function initialize() { if (!desktop) return; listener ??= await listen('sync-changed', () => { void refresh().catch(e => { error.value = errorMessage(e) }) }); await refresh().catch(e => { error.value = errorMessage(e) }) }
  function dispose() { listener?.(); listener = undefined; revision++ }
  async function act(name: string, args?: Record<string, unknown>) { if (working.value) return; working.value = true; error.value = ''; try { const result = await command(name, args); await refresh(); return result } catch (e) { error.value = errorMessage(e); await refresh().catch(() => {}); throw e } finally { working.value = false } }
  async function listDevices() { devices.value = await command<SyncDevice[]>('sync_devices') }
  return { view, devices, working, error, refresh, initialize, dispose, act, listDevices }
})
