import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { FOLD, fold, titleMatches } from '../renderer/src/shell/board'

const rust = readFileSync(
  fileURLToPath(new URL('../../rust/core/src/domain/text.rs', import.meta.url)),
  'utf8',
)

/**
 * The core's fold table, read out of `text.rs`: every `'x'` in an arm's
 * patterns, with the string the arm pushes. The `other => folded.push(other)`
 * arm carries no literal, so it falls through the pattern.
 */
function coreTable(): Map<string, string> {
  const table = new Map<string, string>()
  const arms = rust.matchAll(/^([^=>\n]+)=>\s*folded\.push(?:_str)?\((?:'([^']*)'|"([^"]*)")\),$/gm)
  for (const arm of arms) {
    const patterns = arm[1] ?? ''
    const value = arm[2] ?? arm[3]
    if (value === undefined) continue
    for (const pattern of patterns.matchAll(/'([^'])'/g)) {
      const letter = pattern[1]
      if (letter !== undefined) table.set(letter, value)
    }
  }
  return table
}

describe('the one text comparison', () => {
  it('is the core’s table, entry for entry (ADR 0008: one rule, one place)', () => {
    const core = coreTable()
    expect(core.size).toBeGreaterThan(60)
    expect(new Map(Object.entries(FOLD))).toEqual(core)
  })

  it('folds the letters the core folds and leaves the letters it does not', () => {
    // The gate's two divergences: the core maps ł, and it does not map ķ.
    expect(fold('Łódź')).toBe('lodz')
    expect(fold('ķērns')).toBe('ķerns')
    expect(fold('Crème brûlée')).toBe('creme brulee')
    expect(fold('Ærø')).toBe('aero')
    expect(fold('Straße')).toBe('strasse')
    expect(fold('STRASSE')).toBe('strasse')
  })

  it('drops a combining mark on its own, as the core does', () => {
    expect(fold('e\u0301')).toBe('e')
    // The core checks the original char and lowercases after it, so the dot
    // lowercasing produces on İ stays; the port keeps the same answer.
    expect(fold('İ')).toBe('i\u0307')
  })

  it('matches a needle the same way the core’s contains does', () => {
    expect(titleMatches('lodz', 'Łódź')).toBe(true)
    expect(titleMatches('kerns', 'ķērns')).toBe(false)
  })
})
