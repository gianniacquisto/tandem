import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { scene } from '../../shots/scenes'

/**
 * The Usage face a human reads: the spend the records reported, the arithmetic
 * the ticket rows carry, and one ticket row rolled down to the records behind
 * it. Every figure is the report's own over the fixture home.
 */
test('the usage face, with a ticket row rolled down', async ({ app, window }) => {
  await openTab(window, 'usage')

  await expect(window.locator('.face__title')).toHaveText('Usage')
  await expect(window.locator('.usage__coverage')).toHaveText(/^reported by \d+ of \d+ records$/)
  await expect(window.locator('.usage__spend')).toHaveText(/^\$\d+\.\d{2}$/)

  const picker = window.locator('.usage__segment')
  await expect(picker).toHaveText(['Day', 'Model', 'Agent', 'Ticket'])
  await expect(window.locator('.usage__table thead th').first()).toHaveText('DAY')

  await window.locator('.usage__segment', { hasText: 'Ticket' }).click()
  await expect(window.locator('.usage__table thead th').first()).toHaveText('TICKET')

  const rows = window.locator('.usage__row')
  await expect(rows.first()).toBeVisible()
  const tickets = rows.filter({ hasNotText: 'No ticket' })
  await expect(tickets.first()).toBeVisible()

  await tickets.first().locator('.usage__label').click()

  const roll = window.locator('.usage__roll')
  await expect(roll).toBeVisible()
  await expect(roll.locator('.usage__session').first()).toHaveText(/^ses_/)
  await expect(roll.locator('.usage__record').first()).toContainText(/\d+ in/)

  await capture(app, window, scene('usage'))
})
