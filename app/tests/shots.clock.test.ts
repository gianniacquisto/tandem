import { runInNewContext } from 'node:vm'

import { describe, expect, it } from 'vitest'

import { FIXED_CLOCK, clockScript } from '../shots/clock'

type Pinned = DateConstructor & { now(): number }

function pinnedClock(iso?: string): Pinned {
  const window: { Date?: DateConstructor } = {}
  runInNewContext(clockScript(iso), { window })
  return window.Date as Pinned
}

describe('the shots clock', () => {
  it('pins now, and a no-argument new Date(), to the fixed instant', () => {
    const Pinned = pinnedClock()

    expect(Pinned.now()).toBe(Date.parse(FIXED_CLOCK))
    expect(new Pinned().toISOString()).toBe(FIXED_CLOCK)
  })

  it('keeps a date built from arguments, so a scene formats the report rather than inventing it', () => {
    const Pinned = pinnedClock()

    expect(new Pinned('2026-10-29T08:00:00Z').toISOString()).toBe('2026-10-29T08:00:00.000Z')
    expect(Pinned.parse('2026-10-29T08:00:00Z')).toBe(Date.parse('2026-10-29T08:00:00Z'))
  })

  it('pins the instant it is given, so a scene can name its own', () => {
    expect(pinnedClock('2020-01-02T03:04:05.000Z').now()).toBe(Date.parse('2020-01-02T03:04:05.000Z'))
  })
})
