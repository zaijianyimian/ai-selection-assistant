import type { UsageAggregate, UsageMetric, UsagePurpose } from './types'
export const purposeLabels: Record<UsagePurpose, string> = { translation: '翻译', 'connection-test': '连接测试', 'manual-analysis': '手动主题分析', 'auto-analysis': '自动主题分析' }
export function tokens(value: string | null): string {
  if (value === null || !/^\d+$/.test(value)) return '未知'
  return BigInt(value).toLocaleString('zh-CN')
}
export function metricText(metric: UsageMetric): string {
  return `${tokens(metric.tokens)}${metric.overflow ? '（累计溢出，保留下界）' : metric.unknown && metric.tokens !== null ? '（已报告）' : ''}`
}
export function coverage(a: UsageAggregate): string {
  if (!a.attempts && !a.pending) return '暂无请求记录'
  if (!a.attempts && a.pending) return `${a.pending} 次待完成 · 用量尚未报告`
  const unknown = Math.max(a.input.unknown, a.output.unknown, a.total.unknown)
  const details = unknown ? `未完整报告：输入 ${a.input.unknown} 次、输出 ${a.output.unknown} 次、总量 ${a.total.unknown} 次` : '输入、输出与总量均已报告'
  return `${a.attempts} 次已结束请求 · ${details}${a.pending ? ` · ${a.pending} 次待完成` : ''}`
}
