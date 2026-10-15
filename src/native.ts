import { invoke, isTauri } from '@tauri-apps/api/core'

export const desktop = isTauri()

export async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (!desktop) throw { code: 'browser_preview', message: '浏览器仅用于界面预览。请运行 npm run tauri dev，使用桌面版进行翻译与配置。' }
  return invoke<T>(name, args)
}
