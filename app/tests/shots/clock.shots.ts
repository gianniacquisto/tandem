import { expect } from '@playwright/test'

import { openTab, test } from '../../shots/app'
import { FIXED_CLOCK } from '../../shots/clock'

/**
 * The seed is the fixture home over `rpc` and the fixed clock, asserted in the
 * page rather than in a comment: a scene that renders a now-relative time then
 * renders one instant, on every run. The store revision the core answered reads
 * in the Settings face beside the doctor checks, never in the header.
 */
test('the window runs at the fixed clock, over the store the rpc child read', async ({ window }) => {
  expect(await window.evaluate(() => Date.now())).toBe(Date.parse(FIXED_CLOCK))
  expect(await window.evaluate(() => new Date().toISOString())).toBe(FIXED_CLOCK)
  await expect(window.locator('.shell__revision')).toHaveCount(0)

  await openTab(window, 'settings')
  await expect(
    window.locator('.settings__section', {
      has: window.locator('.settings__heading', { hasText: 'Setup' }),
    }).locator('.settings__versions'),
  ).toContainText(/store rev \d+/)
})
