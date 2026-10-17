import type { UsageAggregate, UsageSnapshot, UsageSummary } from '../types'
export const aggregate = (value: string | null = '130', unknown = 0): UsageAggregate => {
  const field = (tokens: string | null) => ({ tokens, known: tokens === null ? 0 : 1, unknown, overflow: false })
  return { attempts: 1, pending: 0, input: field('100'), output: field('30'), total: field(value), cachedInput: field(null), reasoningOutput: field(null) }
}
export const snapshot = (): UsageSnapshot => ({ operationId: 'fixture-operation', generation: 1, source: 'provider', purpose: 'translation', usage: aggregate(), persistenceIncomplete: false, excludedAfterReset: false })
export const summary = (generation = 1, revision = 1): UsageSummary => ({ generation, revision, since: 1, date: '2026-10-17', incomplete: false, today: aggregate(), month: aggregate(), cumulative: aggregate(), groups: [], groupCount: 0 })
