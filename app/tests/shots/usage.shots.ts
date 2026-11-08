import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

/**
 * The Usage face a human reads: the windowed spend the records reported, the
 * token-first day graph above the picker, the arithmetic the ticket rows carry,
 * and one ticket row rolled down to the records behind it. Every figure is the
 * report's own over the fixture home, read through All time so the frozen days
 * stay in view whatever today is.
 */
test('the usage face, with a ticket row rolled down', async ({ app, window }) => {
  await openTab(window, 'usage')

  await expect(window.locator('.face__title')).toHaveCount(0)
  await expect(window.locator('.usage__spend')).toHaveText(/^\$\d+\.\d{2}$/)

  // The face reads the day palette: its quiet text is --ink-dim.
  expect(await computed(window, '.usage__coverage', 'color')).toBe(token('ink-dim'))

  // The window picker opens on the last seven days; All time keeps the frozen
  // home stable, so the spec reads through it from here on.
  const win = window.locator('.usage__window-segment')
  await expect(win).toHaveText(['Last 7 days', 'Last 30 days', 'This month', 'All time'])
  await expect(window.locator('.usage__window [aria-selected="true"]')).toHaveText('Last 7 days')
  await expect(window.locator('.usage__coverage')).toHaveText(/^reported by \d+ of \d+ records in the last 7 days$/)
  await win.filter({ hasText: 'All time' }).click()
  await expect(window.locator('.usage__window [aria-selected="true"]')).toHaveText('All time')
  await expect(window.locator('.usage__coverage')).toHaveText(/^reported by \d+ of \d+ records$/)
  // The window narrows the report asynchronously: wait for the All time rows
  // before reading them, so a home whose frozen days have aged out of the
  // default seven-day window still reads its own figures here.
  await expect(window.locator('.usage__row').first()).toBeVisible()

  // Three charts, one per unit, all always visible in one row, each with its
  // own quiet heading, its own scale and a swatched legend.
  const charts = window.locator('.usage__chart')
  await expect(charts).toHaveCount(3)
  await expect(charts.locator('.usage__chart-title')).toHaveText(['Tokens', 'Records', 'Cost'])
  // At the shell's 760 px floor the three fit one row without truncation.
  const chartBoxes = await charts.evaluateAll((sections) =>
    sections.map((section) => section.getBoundingClientRect().toJSON()),
  )
  expect(chartBoxes).toHaveLength(3)
  expect(chartBoxes.every((box) => box.width > 0)).toBe(true)
  expect(new Set(chartBoxes.map((box) => Math.round(box.y))).size).toBe(1)
  const tokenChart = charts.nth(0)
  const recordChart = charts.nth(1)
  const costChart = charts.nth(2)

  // All time buckets the day rows into Monday-started weeks: every column is
  // labeled with the week's first day, a Monday's ISO date, and the Day table
  // below still lists the report's day rows.
  const dayRowCount = await window.locator('.usage__row').count()
  expect(dayRowCount).toBeGreaterThan(0)
  for (const chart of [tokenChart, recordChart, costChart]) {
    await expect(chart).toBeVisible()
    // The axis ticks replace the retired prose scales line.
    await expect(chart.locator('.usage__tick')).toHaveCount(2)
    await expect(chart.locator('.usage__gridline')).toHaveCount(2)
    const labels = await chart.locator('.usage__bar-label').allTextContents()
    expect(labels.length).toBeGreaterThan(0)
    for (const label of labels) {
      expect(label).toMatch(/^\d{4}-\d{2}-\d{2}$/)
      expect(new Date(`${label}T00:00:00Z`).getUTCDay()).toBe(1)
    }
  }

  // The tokens chart opens stacked: five piece swatches, total tokens off.
  const series = tokenChart.locator('.usage__series-option')
  await expect(series).toHaveText([
    'Total tokens',
    'Input',
    'Output',
    'Cache read',
    'Cache write',
    'Reasoning',
  ])
  await expect(tokenChart.locator('.usage__series-option[aria-pressed="true"]')).toHaveText([
    'Input',
    'Output',
    'Cache read',
    'Cache write',
    'Reasoning',
  ])
  await expect(tokenChart.locator('.usage__swatch')).toHaveCount(5)
  // Records and Cost are single-series charts: one plain swatch each.
  await expect(recordChart.locator('.usage__swatch')).toHaveCount(1)
  await expect(costChart.locator('.usage__swatch')).toHaveCount(1)

  // Each drawn slice names its column, its piece and its value.
  for (const name of await tokenChart.getByRole('img').evaluateAll((bars) =>
    bars.map((bar) => bar.getAttribute('aria-label')),
  )) {
    expect(name).toMatch(
      /^\d{4}-\d{2}-\d{2} (?:Input|Output|Cache read|Cache write|Reasoning): [\d,]+$/,
    )
  }
  for (const name of await recordChart.getByRole('img').evaluateAll((bars) =>
    bars.map((bar) => bar.getAttribute('aria-label')),
  )) {
    expect(name).toMatch(/^\d{4}-\d{2}-\d{2} Records: [\d,]+$/)
  }
  for (const name of await costChart.getByRole('img').evaluateAll((bars) =>
    bars.map((bar) => bar.getAttribute('aria-label')),
  )) {
    expect(name).toMatch(/^\d{4}-\d{2}-\d{2} Cost: \$\d+\.\d{2}$/)
  }

  // Tick text is the scale statement in each unit: tokens and record counts
  // grouped, dollars with two decimals.
  for (const tick of await tokenChart.locator('.usage__tick').allTextContents()) {
    expect(tick).toMatch(/^[\d,]+$/)
  }
  for (const tick of await recordChart.locator('.usage__tick').allTextContents()) {
    expect(tick).toMatch(/^[\d,]+$/)
  }
  for (const tick of await costChart.locator('.usage__tick').allTextContents()) {
    expect(tick).toMatch(/^\$\d+\.\d{2}$/)
  }

  // The legend round-trip: total tokens swaps the stack for one total bar and
  // its swatch, then off restores the stack.
  const totalOption = series.filter({ hasText: 'Total tokens' })
  await totalOption.click()
  await expect(totalOption).toHaveAttribute('aria-pressed', 'true')
  await expect(tokenChart.locator('.usage__swatch')).toHaveCount(6)
  await expect(tokenChart.getByRole('img').first()).toHaveAttribute(
    'aria-label',
    /^\d{4}-\d{2}-\d{2} Total tokens: [\d,]+$/,
  )
  await totalOption.click()
  await expect(totalOption).toHaveAttribute('aria-pressed', 'false')
  await expect(tokenChart.locator('.usage__swatch')).toHaveCount(5)
  await expect(tokenChart.getByRole('img').first()).toHaveAttribute(
    'aria-label',
    /^\d{4}-\d{2}-\d{2} Input: [\d,]+$/,
  )

  const chartBox = await window.locator('.usage__charts').boundingBox()
  const pickerBox = await window.locator('.usage__picker').boundingBox()
  expect(chartBox === null || pickerBox === null).toBe(false)
  if (chartBox !== null && pickerBox !== null) {
    expect(chartBox.y + chartBox.height).toBeLessThanOrEqual(pickerBox.y)
  }
  expect(await computed(window, '.usage__bar--records', 'background-color')).toBe(token('ink-dim'))
  expect(await computed(window, '.usage__bar-label', 'color')).toBe(token('ink-dim'))
  expect(await computed(window, '.usage__chart-title', 'color')).toBe(token('ink-dim'))

  const picker = window.locator('.usage__segment')
  await expect(picker).toHaveText(['Day', 'Model', 'Agent', 'Ticket'])
  await expect(window.locator('.usage__table thead th').first()).toHaveText('DAY')

  // The Total/Average toggle beside the picker, defaulting to totals, and the
  // cache columns the table carries in both modes.
  const mode = window.locator('.usage__mode-segment')
  await expect(mode).toHaveText(['Total', 'Average'])
  await expect(window.locator('.usage__mode [aria-selected="true"]')).toHaveText('Total')
  const headers = window.locator('.usage__table thead th')
  await expect(headers.filter({ hasText: 'Cache read' })).toHaveCount(1)
  await expect(headers.filter({ hasText: 'Cache write' })).toHaveCount(1)

  // Switching to Average rewrites the frozen day's In cell to its
  // per-reported-record average: 3,000 over 2 reported records.
  const frozenDay = window.locator('.usage__row', { hasText: '2026-10-30' })
  const inCell = frozenDay.locator('td').nth(1)
  await expect(inCell).toHaveText('3,000')
  await mode.filter({ hasText: 'Average' }).click()
  await expect(window.locator('.usage__mode [aria-selected="true"]')).toHaveText('Average')
  await expect(inCell).toHaveText('1,500')
  await expect(frozenDay.locator('td').nth(5)).toHaveText('$0.50')
  await mode.filter({ hasText: 'Total' }).click()
  await expect(inCell).toHaveText('3,000')

  await window.locator('.usage__segment', { hasText: 'Ticket' }).click()
  await expect(window.locator('.usage__table thead th').first()).toHaveText('TICKET ↑')

  const rows = window.locator('.usage__row')
  await expect(rows.first()).toBeVisible()
  const tickets = rows.filter({ hasNotText: 'No ticket' })
  await expect(tickets.first()).toBeVisible()

  await tickets.first().locator('.usage__label').click()

  const roll = window.locator('.usage__roll')
  await expect(roll).toBeVisible()
  await expect(roll.locator('.usage__session').first()).toHaveText(/^ses_/)
  await expect(roll.locator('.usage__record').first()).toContainText(/\d+ in/)

  // The selection survives the mode switch: the rolled-down records stay open.
  await mode.filter({ hasText: 'Average' }).click()
  await expect(roll).toBeVisible()
  await mode.filter({ hasText: 'Total' }).click()
  await expect(roll).toBeVisible()

  await capture(app, window, scene('usage'))
})
