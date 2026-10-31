/**
 * The clock every scene is photographed at.
 *
 * The seed is the fixture home plus this instant: the home fixes the data, and
 * the clock fixes whatever a scene renders relative to now, so two runs of the
 * same checkout write the same picture. It is a real instant from the fixture's
 * own day (2026-10-30), so a scene that prints a date prints one the fixture
 * store could have written.
 */
export const FIXED_CLOCK = '2026-10-30T12:00:00.000Z'

/**
 * The init script that pins a page's clock before any application script runs:
 * `Date.now()` and a no-argument `new Date()` answer [`FIXED_CLOCK`]. `new
 * Date(<args>)` and `Date.parse` keep the real behaviour, because a scene
 * formats the report's timestamps and never invents one.
 *
 * It is a string rather than a function because Playwright serializes a
 * function's source but not the typed closure around it, and the runner loads
 * this tree as CommonJS (`roots.ts`).
 */
export function clockScript(iso: string = FIXED_CLOCK): string {
  return `(() => {
  const fixed = Date.parse(${JSON.stringify(iso)})
  const Real = Date
  class Pinned extends Real {
    constructor(...args) {
      if (args.length === 0) {
        super(fixed)
      } else {
        super(...args)
      }
    }
    static now() {
      return fixed
    }
  }
  Object.defineProperty(window, 'Date', { value: Pinned, writable: true, configurable: true })
})()`
}
