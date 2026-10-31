import { describe, expect, it } from 'vitest'

import { isFixtureHome } from '../src/main/shot'

const root = '/repo/Fixtures'

describe('the screenshot rule', () => {
  it('accepts a home under the repository fixtures', () => {
    expect(isFixtureHome('/repo/Fixtures/port/Home', root)).toBe(true)
    expect(isFixtureHome('/repo/Fixtures', root)).toBe(true)
  })

  it('refuses a real home, a sibling prefix, and no home at all', () => {
    expect(isFixtureHome('/Users/someone/.tandem', root)).toBe(false)
    expect(isFixtureHome('/repo/Fixtures-other/Home', root)).toBe(false)
    expect(isFixtureHome(undefined, root)).toBe(false)
    expect(isFixtureHome('', root)).toBe(false)
  })
})
