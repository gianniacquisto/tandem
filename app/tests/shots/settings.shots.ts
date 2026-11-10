import { existsSync, mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { appPath, seedPath } from '../../shots/home'
import { screenshotPath } from '../../shots/output'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

/**
 * The Settings face a human reads: the machine's paths (the seeded home and
 * the seeded user home's three hosts) with one-click Copy, the Preferences
 * theme rule, and the setup report's thirteen checks in four groups with their
 * state.
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

  // The Setup line is host-free: a debug build bundles no face and the seeded
  // home has installed none.
  await expect(setup.locator('.settings__versions')).toContainText('Installed nothing; bundled nothing')

  // The app row names the seeded fixture app, its pinned version and no
  // quarantine clause, so the picture carries nothing of this machine's install.
  const appRow = setup.locator('.settings__check', {
    has: window.locator('.settings__check-name', { hasText: /^app$/ }),
  })
  await expect(appRow.locator('.settings__detail')).toHaveText(`installed at ${appPath()} (version 1.0.0)`)
  await expect(appRow.locator('.settings__detail')).not.toContainText('quarantin')

  await capture(app, window, scene('settings'))

  // The run writes its picture into its own output directory; the committed
  // folder moves only through `npm run shots:publish`.
  const written = screenshotPath(process.env, REPO_ROOT, scene('settings'))
  expect(written).not.toBe(join(REPO_ROOT, 'docs', 'screenshots', 'settings-face.png'))
  expect(existsSync(written)).toBe(true)
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
