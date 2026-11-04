import { mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { seedPath } from '../../shots/home'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

/**
 * The Settings face a human reads: the machine's paths (the seeded home and
 * the seeded user home's three hosts) with one-click Copy, the Preferences
 * theme rule, the setup report's thirteen checks in four groups with their
 * state, and the harness sign-in section.
 *
 * The paths are the seed's own, at the one fixed scratch root, so the picture
 * is the same in any checkout. What the rule pins is that they are the seed's
 * paths, never a human's.
 */
test('the settings face, over the seeded homes', async ({ app, window }) => {
  await openTab(window, 'settings')

  await expect(window.locator('.face__title')).toHaveCount(0)
  // The store revision the core answered reads beside the doctor checks,
  // never in the header.
  await expect(
    window.locator('.settings__section', {
      has: window.locator('.settings__heading', { hasText: 'Setup' }),
    }).locator('.settings__versions'),
  ).toContainText(/store rev \d+/)
  await expect(window.locator('.settings__heading')).toHaveText([
    'Paths',
    'Preferences',
    'Roster',
    'Setup',
    'Harness sign-in',
  ])

  const paths = window.locator('.settings__section', {
    has: window.locator('.settings__heading', { hasText: 'Paths' }),
  })
  const preferences = window.locator('.settings__section', {
    has: window.locator('.settings__heading', { hasText: 'Preferences' }),
  })
  const facts = paths.locator('.settings__fact')
  await expect(facts.first()).toContainText('Home')
  await expect(facts.first().locator('.settings__value')).toHaveText(/\/tmp\/tandem-shots\/Home$/)
  await expect(facts.nth(1).locator('.settings__value')).toHaveText(/\/tmp\/tandem-shots\/Home\/bikeshed$/)

  await expect(paths.locator('.settings__term')).toHaveText([
    'Home',
    'Bikeshed',
    'opencode',
    'claude',
    'agents',
  ])
  await expect(facts.nth(2).locator('.settings__value')).toHaveText(
    /\/tmp\/tandem-shots\/user\/\.config\/opencode$/,
  )
  await expect(facts.nth(3).locator('.settings__value')).toHaveText(/\/tmp\/tandem-shots\/user\/\.claude$/)
  await expect(facts.nth(4).locator('.settings__value')).toHaveText(/\/tmp\/tandem-shots\/user\/\.agents$/)

  // Theme lives in Preferences, not Paths, and every path copies in one click.
  await expect(preferences.locator('.settings__term')).toHaveText(['Theme'])
  await expect(preferences.locator('.settings__value')).toHaveText(
    'System: the app follows the OS appearance',
  )
  await expect(paths.getByRole('button', { name: /Copy .* path/ })).toHaveCount(5)

  // The facts panel is a token card: its corner is --radius-card.
  expect(await computed(window, '.settings__facts', 'border-radius')).toBe(token('radius-card'))

  // The checks and the repair control are the Setup section's own: the Roster
  // section above carries its own list and its own Add and Save.
  const setup = window.locator('.settings__section', {
    has: window.locator('.settings__heading', { hasText: 'Setup' }),
  })
  await expect(setup.locator('.settings__check')).toHaveCount(13)
  await expect(setup.locator('.settings__subheading')).toHaveText([
    'Store',
    'Bikeshed',
    'Agent face',
    'Install state',
  ])
  await expect(setup.locator('.settings__hint')).toContainText('Nothing is written until you press it')
  await expect(setup.locator('.settings__action')).toHaveText(/^(Install|Repair)$/)

  // The harness sign-in section offers any provider, not only the Console:
  // the device-flow button, a provider field and a masked key field beside it.
  const signIn = window.locator('.settings__section', {
    has: window.locator('.settings__heading', { hasText: 'Harness sign-in' }),
  })
  await expect(
    signIn.getByRole('button', { name: 'Sign in with OpenCode Console' }),
  ).toBeVisible()
  await expect(signIn.getByRole('button', { name: 'Sign in with API key' })).toBeVisible()
  await expect(signIn.locator('input[type="text"]')).toHaveCount(1)
  await expect(signIn.locator('input[type="password"]')).toHaveCount(1)

  await capture(app, window, scene('settings'))
})

/**
 * The Roster section over the seeded home: the default names (the seed stores
 * none), an add that joins the draft, and one Save that writes the whole list
 * through `settings.set`. The scratch home is reseeded before every test, so
 * this spec's write cannot reach another spec's picture.
 */
test('the settings roster edits round-trip through the store', async ({ app, window }) => {
  await openTab(window, 'settings')

  const roster = window.locator('.settings__section', {
    has: window.locator('.settings__heading', { hasText: 'Roster' }),
  })
  const names = [
    'human',
    'builder',
    'refiner',
    'reviewer',
    'consolidator',
    'sprint',
  ]
  await expect(roster.locator('.settings__check-name')).toHaveText(names)

  // Add joins the draft; nothing reaches the store until Save.
  await roster.locator('.settings__name').fill('nova')
  await roster.getByRole('button', { name: 'Add' }).click()
  await expect(roster.locator('.settings__check-name')).toHaveText([...names, 'nova'])

  await roster.getByRole('button', { name: 'Save roster' }).click()
  await expect(roster.locator('.settings__check-name')).toHaveText([...names, 'nova'])

  // The write landed: a fresh read of the tab answers the stored list.
  await openTab(window, 'tickets')
  await openTab(window, 'settings')
  await expect(roster.locator('.settings__check-name')).toHaveText([...names, 'nova'])
})

/**
 * A refused roster write keeps the draft and shows the error row: the face's
 * existing `role="alert"` row, with the core's own words. The refusal is real:
 * the seeded database is replaced by a directory behind the running app, so
 * the write is refused by the store and nothing reaches it.
 */
test('a refused roster write shows the error row', async ({ app, window }) => {
  await openTab(window, 'settings')

  const roster = window.locator('.settings__section', {
    has: window.locator('.settings__heading', { hasText: 'Roster' }),
  })
  await roster.locator('.settings__name').fill('nova')
  await roster.getByRole('button', { name: 'Add' }).click()

  const database = join(seedPath(), 'tandem.db')
  rmSync(database, { recursive: true, force: true })
  mkdirSync(database, { recursive: true })

  await roster.getByRole('button', { name: 'Save roster' }).click()

  await expect(window.getByRole('alert')).not.toBeEmpty()
  await expect(roster.getByRole('button', { name: 'Remove nova' })).toBeVisible()
})
