export interface Settings {
  baseUrl: string
  model: string
  temperature: number
  timeoutSecs: number
  shortcut: string
  explainTerms: boolean
  outputMode: 'auto' | 'json' | 'prompt'
  captureMode: 'auto' | 'clipboard'
  theme: 'system' | 'light' | 'dark'
  targetLanguage: '简体中文' | 'English' | '日本語'
  rememberTerms: boolean
  personalizeExplanations: boolean
  autoAnalyze: boolean
  explanationStyle: 'balanced' | 'introductory' | 'concise' | 'practical'
  backgroundDomain: 'general' | 'backend' | 'ai' | 'database'
}
export interface AppError { code: string; message: string }
export interface Term { original: string; translation: string; domain: string; explanation: string; example: string | null; senseSummary?: string; memoryId?: number | null }
export interface Translation { translation: string; sourceLanguage: string; terms: Term[]; compatibilityFallback: boolean }
export interface Job {
  revision: number
  status: 'idle' | 'capturing' | 'translating' | 'success' | 'error'
  original: string
  source: string
  warning: string | null
  result: Translation | null
  error: AppError | null
}
export interface SettingsView { settings: Settings; hasApiKey: boolean; warnings: string[] }
export interface ConnectionTest { message: string; elapsedMs: number; compatibilityFallback: boolean }

export const defaultSettings = (): Settings => ({
  baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini', temperature: 0.2,
  timeoutSecs: 45, shortcut: 'Ctrl+Shift+Q', explainTerms: true, outputMode: 'auto',
  captureMode: 'auto', theme: 'system', targetLanguage: '简体中文',
  rememberTerms: false, personalizeExplanations: false, autoAnalyze: false,
  explanationStyle: 'balanced', backgroundDomain: 'general',
})

export type Familiarity = 'unknown' | 'learning' | 'known'
export interface StoredTerm { id: number; original: string; translation: string; domain: string; senseSummary: string; language: string; count: number; lastSeen: number; familiarity: Familiarity }
export interface ProfileVersion { id: number; createdAt: number; profile: { topics: { domain: string; evidenceIds: number[] }[] } }
export interface MemoryView { terms: StoredTerm[]; total: number; pending: number; versions: ProfileVersion[]; activeVersion: number; lastAutoAttempt: number }
export interface AnalysisStatus { running: boolean; message: string; error: AppError | null }

export function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error && typeof error.message === 'string') return error.message
  if (typeof error === 'string') return error
  return '操作失败，请稍后重试。'
}
