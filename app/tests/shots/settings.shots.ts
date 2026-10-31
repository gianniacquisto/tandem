import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { scene } from '../../shots/scenes'

/**
 * The Settings face a human reads: the machine's paths (the seeded home and
 * the seeded user home's three hosts), the one theme rule, and the setup
 * report's eight checks with their state.
 *
 * The paths are the seed's own, at the one fixed scratch root, so the picture
 * is the same in any checkout. What the rule pins is that they are the seed's
 * paths, never a human's.
 */
test('the settings face, over the seeded homes', async ({ app, window }) => {
  await openTab(window, 'settings')

  await expect(window.locator('.face__title')).toHaveText('Settings')
  await expect(window.locator('.settings__heading')).toHaveText(['Paths', 'Setup'])

  const facts = window.locator('.settings__fact')
  await expect(facts.first()).toContainText('Home')
  await expect(facts.first().locator('.settings__value')).toHaveText(/\/tmp\/tandem-shots\/Home$/)
  await expect(facts.nth(1).locator('.settings__value')).toHaveText(/\/tmp\/tandem-shots\/Home\/bikeshed$/)

  await expect(window.locator('.settings__term')).toHaveText([
    'Home',
    'Bikeshed',
    'opencode',
    'claude',
    'agents',
    'Theme',
  ])
  await expect(facts.nth(2).locator('.settings__value')).toHaveText(
    /\/tmp\/tandem-shots\/user\/\.config\/opencode$/,
  )
  await expect(facts.nth(3).locator('.settings__value')).toHaveText(/\/tmp\/tandem-shots\/user\/\.claude$/)
  await expect(facts.nth(4).locator('.settings__value')).toHaveText(/\/tmp\/tandem-shots\/user\/\.agents$/)

  await expect(window.locator('.settings__checks').first().locator('.settings__check')).toHaveCount(8)
  await expect(window.locator('.settings__action')).toHaveText(/^(Install|Repair)$/)

  await capture(app, window, scene('settings'))
})
