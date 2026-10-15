import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { expect, it } from 'vitest'

it('bundles local glyph coverage for every Chinese interface character with valid UTF-8 sources', () => {
  const css = readFileSync('node_modules/@fontsource-variable/noto-sans-sc/index.css', 'utf8')
  const ranges = [...css.matchAll(/U\+([0-9a-f]+)(?:-([0-9a-f]+))?/gi)]
    .map(match => [parseInt(match[1]!, 16), parseInt(match[2] ?? match[1]!, 16)] as const)
  const files = (directory: string): string[] => readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name)
    return entry.isDirectory() ? files(path) : /\.(vue|ts|rs)$/.test(path) ? [path] : []
  })
  const missing = new Set<string>()
  for (const path of [...files('src'), ...files('src-tauri/src')]) {
    const source = new TextDecoder('utf-8', { fatal: true }).decode(readFileSync(path))
    expect(source, `${path} contains a Unicode replacement character`).not.toContain('\uFFFD')
    for (const character of source.match(/[\u4e00-\u9fff]/g) ?? []) {
      const point = character.codePointAt(0)!
      if (!ranges.some(([start, end]) => point >= start && point <= end)) missing.add(character)
    }
  }
  expect([...missing], 'bundled font must cover Chinese labels and native error messages').toEqual([])
})
