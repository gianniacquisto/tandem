import { expect, type Locator } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

/** Each drawn bar's colour, by the series its aria label names. */
async function barColours(chart: Locator): Promise<Record<string, string>> {
  const bars = await chart.locator('[role="img"]').evaluateAll((elements) =>
    elements.map((element) => {
      const label = element.getAttribute('aria-label') ?? ''
      return {
        series: label.replace(/^\d{4}-\d{2}-\d{2} /, '').replace(/:.*$/, ''),
        colour: getComputedStyle(element).backgroundColor,
      }
    }),
  )
  const colours: Record<string, string> = {}
  for (const bar of bars) colours[bar.series] ??= bar.colour
  for (const bar of bars) expect(bar.colour).toBe(colours[bar.series])
  return colours
}

/** Each legend swatch's colour, by the series its text names. */
async function swatchColours(chart: Locator): Promise<Record<string, string>> {
  const swatches = await chart.locator('.usage__legend-item, .usage__series-option').evaluateAll(
    (elements) =>
      elements.map((element) => {
        const swatch = element.querySelector('.usage__swatch')
        return {
          series: element.textContent?.trim() ?? '',
          colour: swatch === null ? '' : getComputedStyle(swatch).backgroundColor,
        }
      }),
  )
  const colours: Record<string, string> = {}
  for (const swatch of swatches) colours[swatch.series] = swatch.colour
  return colours
}

/**
 * The Usage face a human reads: the windowed spend the records reported, the
 * four per-scale charts above the picker, the arithmetic the ticket rows carry,
 * and one ticket row rolled down to the records behind it. Every figure is the
 * report's own over the fixture home, read through All time so the frozen weeks
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

  // Four charts, one per scale, all always visible, each with its own quiet
  // heading, its own scale and a swatched legend.
  const charts = window.locator('.usage__chart')
  await expect(charts).toHaveCount(4)
  await expect(charts.locator('.usage__chart-title')).toHaveText([
    'Tokens',
    'Cache',
    'Total tokens',
    'Cost',
  ])
  const chartBoxes = await charts.evaluateAll((sections) =>
    sections.map((section) => section.getBoundingClientRect().toJSON()),
  )
  expect(chartBoxes).toHaveLength(4)
  expect(chartBoxes.every((box) => box.width > 0 && box.height > 0)).toBe(true)
  const tokenChart = charts.nth(0)
  const cacheChart = charts.nth(1)
  const totalChart = charts.nth(2)
  const costChart = charts.nth(3)

  // All time buckets the day rows into Monday-started weeks: every column is
  // labeled with the week's first day, a Monday's ISO date, and the Day table
  // below still lists the report's day rows.
  const dayRowCount = await window.locator('.usage__row').count()
  expect(dayRowCount).toBeGreaterThan(0)
  for (const chart of [tokenChart, cacheChart, totalChart, costChart]) {
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

  // Tokens groups input and output side by side; Cache groups cache read and
  // write. A column draws one bar per piece with a value, never a stacked slice.
  const tokenSeries = tokenChart.locator('.usage__series-option')
  await expect(tokenSeries).toHaveText(['Input', 'Output'])
  await expect(tokenChart.locator('.usage__series-option[aria-pressed="true"]')).toHaveText([
    'Input',
    'Output',
  ])
  await expect(tokenChart.locator('.usage__swatch')).toHaveCount(2)
  const cacheSeries = cacheChart.locator('.usage__series-option')
  await expect(cacheSeries).toHaveText(['Cache read', 'Cache write'])
  await expect(cacheChart.locator('.usage__series-option[aria-pressed="true"]')).toHaveText([
    'Cache read',
    'Cache write',
  ])
  await expect(cacheChart.locator('.usage__swatch')).toHaveCount(2)
  // Total tokens and Cost are single-series charts: one plain swatch each.
  await expect(totalChart.locator('.usage__swatch')).toHaveCount(1)
  await expect(totalChart.locator('.usage__legend-items')).toHaveText('Total tokens')
  await expect(costChart.locator('.usage__swatch')).toHaveCount(1)
  await expect(costChart.locator('.usage__legend-items')).toHaveText('Cost')

  // The legend draws exactly the colour its bars do, and the two grouped charts
  // share one pair: the first piece of each group reads one colour and the
  // second another, the same assignment in both charts.
  const tokenBars = await barColours(tokenChart)
  const cacheBars = await barColours(cacheChart)
  const totalBars = await barColours(totalChart)
  const costBars = await barColours(costChart)
  const tokenSwatches = await swatchColours(tokenChart)
  const cacheSwatches = await swatchColours(cacheChart)
  const totalSwatches = await swatchColours(totalChart)
  const costSwatches = await swatchColours(costChart)
  expect(tokenSwatches.Input).toBe(tokenBars.Input)
  expect(tokenSwatches.Output).toBe(tokenBars.Output)
  expect(cacheSwatches['Cache read']).toBe(cacheBars['Cache read'])
  expect(cacheSwatches['Cache write']).toBe(cacheBars['Cache write'])
  expect(totalSwatches['Total tokens']).toBe(totalBars['Total tokens'])
  expect(costSwatches.Cost).toBe(costBars.Cost)
  expect(tokenBars.Input).toBe(cacheBars['Cache read'])
  expect(tokenBars.Output).toBe(cacheBars['Cache write'])
  expect(tokenBars.Input).not.toBe(tokenBars.Output)
  expect(tokenBars.Input).toBe(token('accent'))
  expect(tokenBars.Output).toBe(token('ember'))
  expect(totalBars['Total tokens']).toBe(token('accent-deep'))
  expect(costBars.Cost).toBe(token('ember-deep'))

  // Every column owns one bar group: the grouped charts draw one to two bars
  // per group, the single-series charts exactly one. Nothing stacks.
  for (const chart of [tokenChart, cacheChart, totalChart, costChart]) {
    const columnCount = await chart.locator('.usage__bar-label').count()
    const groups = chart.locator('.usage__bar-group')
    await expect(groups).toHaveCount(columnCount)
  }
  for (const chart of [tokenChart, cacheChart]) {
    const groups = chart.locator('.usage__bar-group')
    for (let index = 0; index < (await groups.count()); index += 1) {
      const count = await groups.nth(index).getByRole('img').count()
      expect(count).toBeGreaterThanOrEqual(1)
      expect(count).toBeLessThanOrEqual(2)
    }
  }
  for (const chart of [totalChart, costChart]) {
    const counts = await chart
      .locator('.usage__bar-group')
      .evaluateAll((groups) => groups.map((group) => group.querySelectorAll('[role="img"]').length))
    expect(counts.every((count) => count === 1)).toBe(true)
  }

  // Each drawn bar names its column, its piece and its value. The records
  // chart and the reasoning bar are gone.
  for (const name of await tokenChart.getByRole('img').evaluateAll((bars) =>
    bars.map((bar) => bar.getAttribute('aria-label')),
  )) {
    expect(name).toMatch(/^\d{4}-\d{2}-\d{2} (?:Input|Output): [\d,]+$/)
  }
  for (const name of await cacheChart.getByRole('img').evaluateAll((bars) =>
    bars.map((bar) => bar.getAttribute('aria-label')),
  )) {
    expect(name).toMatch(/^\d{4}-\d{2}-\d{2} (?:Cache read|Cache write): [\d,]+$/)
  }
  for (const name of await totalChart.getByRole('img').evaluateAll((bars) =>
    bars.map((bar) => bar.getAttribute('aria-label')),
  )) {
    expect(name).toMatch(/^\d{4}-\d{2}-\d{2} Total tokens: [\d,]+$/)
  }
  for (const name of await costChart.getByRole('img').evaluateAll((bars) =>
    bars.map((bar) => bar.getAttribute('aria-label')),
  )) {
    expect(name).toMatch(/^\d{4}-\d{2}-\d{2} Cost: \$\d+\.\d{2}$/)
  }

  // Tick text is the scale statement: tokens grouped, dollars with two decimals.
  for (const chart of [tokenChart, cacheChart, totalChart]) {
    for (const tick of await chart.locator('.usage__tick').allTextContents()) {
      expect(tick).toMatch(/^[\d,]+$/)
    }
  }
  for (const tick of await costChart.locator('.usage__tick').allTextContents()) {
    expect(tick).toMatch(/^\$\d+\.\d{2}$/)
  }

  // The legend round-trip: turning a piece off drops its bars and its swatch,
  // and turning it back on restores both.
  const inputOption = tokenSeries.filter({ hasText: 'Input' })
  await inputOption.click()
  await expect(inputOption).toHaveAttribute('aria-pressed', 'false')
  await expect(tokenChart.locator('.usage__swatch')).toHaveCount(1)
  await expect(tokenChart.getByRole('img').first()).toHaveAttribute(
    'aria-label',
    /^\d{4}-\d{2}-\d{2} Output: [\d,]+$/,
  )
  await inputOption.click()
  await expect(inputOption).toHaveAttribute('aria-pressed', 'true')
  await expect(tokenChart.locator('.usage__swatch')).toHaveCount(2)
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
