import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

/**
 * The Usage face a human reads: the spend the records reported, the cost-per-day
 * bar chart above the picker, the arithmetic the ticket rows carry, and one
 * ticket row rolled down to the records behind it. Every figure is the report's
 * own over the fixture home.
 */
test('the usage face, with a ticket row rolled down', async ({ app, window }) => {
  await openTab(window, 'usage')

  await expect(window.locator('.face__title')).toHaveText('Usage')
  await expect(window.locator('.usage__coverage')).toHaveText(/^reported by \d+ of \d+ records$/)
  await expect(window.locator('.usage__spend')).toHaveText(/^\$\d+\.\d{2}$/)

  // The face reads the day palette: its quiet text is --ink-dim.
  expect(await computed(window, '.usage__coverage', 'color')).toBe(token('ink-dim'))

  // The chart draws one bar per day row, above the picker, each named with its
  // day and dollar figure. The bars reuse the meter's fill token.
  const dayRowCount = await window.locator('.usage__row').count()
  const chart = window.locator('.usage__chart')
  await expect(chart).toBeVisible()
  const bars = chart.getByRole('img')
  await expect(bars).toHaveCount(dayRowCount)
  const found = await bars.all()
  const names = await Promise.all(found.map((bar) => bar.getAttribute('aria-label')))
  expect(names).toHaveLength(dayRowCount)
  for (const name of names) {
    expect(name).toMatch(/^.+?: (?:\$[\d,]+\.\d{2}|\u2014)$/)
  }
  const chartBox = await chart.boundingBox()
  const pickerBox = await window.locator('.usage__picker').boundingBox()
  expect(chartBox === null || pickerBox === null).toBe(false)
  if (chartBox !== null && pickerBox !== null) {
    expect(chartBox.y + chartBox.height).toBeLessThanOrEqual(pickerBox.y)
  }
  expect(await computed(window, '.usage__bar', 'background-color')).toBe(token('accent'))
  expect(await computed(window, '.usage__bar-label', 'color')).toBe(token('ink-dim'))

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
