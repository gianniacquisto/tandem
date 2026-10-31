import { expect } from '@playwright/test'

import { test } from '../../shots/app'
import { FIXED_CLOCK } from '../../shots/clock'

/**
 * The seed is the fixture home over `rpc` and the fixed clock, asserted in the
 * page rather than in a comment: a scene that renders a now-relative time then
 * renders one instant, on every run.
 */
test('the window runs at the fixed clock, over the store the rpc child read', async ({ window }) => {
  expect(await window.evaluate(() => Date.now())).toBe(Date.parse(FIXED_CLOCK))
  expect(await window.evaluate(() => new Date().toISOString())).toBe(FIXED_CLOCK)
  await expect(window.locator('.shell__revision')).toHaveText(/^rev \d+$/)
})
