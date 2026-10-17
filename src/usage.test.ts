import { describe, expect, it } from 'vitest'
import { coverage, metricText, tokens } from './usage'
import { aggregate } from './test/usage'
describe('Token presentation', () => {
  it('preserves large integers, explicit zero and unknown values', () => {
    expect(tokens('9007199254740993')).toBe('9,007,199,254,740,993')
    expect(tokens('0')).toBe('0'); expect(tokens(null)).toBe('未知')
    expect(tokens('1.5')).toBe('未知')
    expect(metricText({ tokens: '12', known: 1, unknown: 1, overflow: false })).toContain('已报告')
    expect(metricText({ tokens: '12', known: 1, unknown: 0, overflow: true })).toContain('保留下界')
  })
  it('separates empty history, pending and incomplete coverage', () => {
    const a = aggregate(null, 1)
    expect(coverage(a)).toContain('未完整报告')
    a.attempts = 0; a.pending = 1
    expect(coverage(a)).toContain('待完成')
    a.pending = 0; expect(coverage(a)).toBe('暂无请求记录')
  })
})
