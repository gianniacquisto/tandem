import { execFileSync } from 'node:child_process'
import { join } from 'node:path'

import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { seedPath } from '../../shots/home'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

const MOD = process.platform === 'darwin' ? 'Meta' : 'Control'

function binary(): string {
  return join(
    REPO_ROOT,
    'rust',
    'target',
    'debug',
    process.platform === 'win32' ? 'tandem.exe' : 'tandem',
  )
}

/** A second writer: the CLI in a terminal, on the seeded store the app is reading. */
function cli(args: string[]): void {
  execFileSync(binary(), args, {
    env: { ...process.env, TANDEM_HOME: seedPath(), TANDEM_AGENT: 'shots' },
    stdio: 'pipe',
  })
}

/** The same store the app reads, through the CLI: the count the face must draw. */
function cliJson(args: string[]): unknown {
  return JSON.parse(
    execFileSync(binary(), args, {
      env: { ...process.env, TANDEM_HOME: seedPath(), TANDEM_AGENT: 'shots' },
      encoding: 'utf8',
    }),
  )
}

async function rowIds(window: import('@playwright/test').Page): Promise<Array<string | null>> {
  // Read each row through its locator rather than an in-page evaluate: the node
  // project carries no DOM lib, and `@types/react`'s empty globals fill the
  // name, so a `row.getAttribute` callback cannot be typed.
  const rows = await window.locator('.ticket-row').all()
  return Promise.all(rows.map((row) => row.getAttribute('data-ticket')))
}

/** The board cell a card sits in: its lane key (the default area is the empty string) and status. */
async function cellOf(
  window: import('@playwright/test').Page,
  ticket: string,
): Promise<{ lane: string | null; status: string | null }> {
  const cell = window
    .locator('.board__cell', {
      has: window.locator(`.ticket-row[data-ticket="${ticket}"]`),
    })
    .first()
  return { lane: await cell.getAttribute('data-lane'), status: await cell.getAttribute('data-status') }
}

test('the tickets face shows one board two ways: the list and the board over one snapshot', async ({
  app,
  window,
}) => {
  await openTab(window, 'tickets')

  await expect(window.locator('.face__title')).toHaveText('Tickets')
  await expect(window.locator('.quick-add__input')).toHaveAttribute(
    'placeholder',
    'Add a ticket…',
  )
  await expect(window.locator('.search__input')).toHaveAttribute('placeholder', 'Search tickets…')

  // The shipped scope opens on Open; All includes the archive and is the shape
  // both layouts must agree on. The count is the store's own, read through the
  // CLI: the face draws every row the store holds.
  const store = cliJson(['ticket', 'list', '--filter', 'all', '--archived']) as {
    tickets: Array<{ id: string }>
  }
  const storeIds = store.tickets.map((one) => one.id)
  if ((process.env['TANDEM_SHOTS_HOME'] ?? '').endsWith('Fixtures/port/Board')) {
    expect(storeIds.length).toBe(78)
  }

  await window.getByRole('tab', { name: /^All/ }).click()
  await expect(window.getByRole('tab', { name: /^All/ })).toContainText(String(storeIds.length))
  const listIds = await rowIds(window)
  expect(listIds.length).toBe(storeIds.length)
  expect([...listIds].sort()).toEqual([...storeIds].sort())

  // A title search that matches nothing says so, in the face's own words.
  await window.locator('.search__input').fill('zzzz no such ticket')
  await expect(window.locator('.empty__title')).toHaveText('No matches')
  await window.locator('.search__clear').click()
  await expect(window.locator('.search__input')).toHaveValue('')
  await expect(window.locator('.ticket-row').first()).toBeVisible()

  // The same rows, laid out as lane rows and status columns: same tickets, cut
  // the other way.
  await window.getByRole('tab', { name: 'Board' }).click()
  await expect(window.locator('.board')).toBeVisible()
  await expect(window.locator('.board__cell').first()).toBeVisible()
  await expect(window.locator('.board__status')).toHaveCount(6)
  // The parked status is the last column, not one among the workflow ones.
  await expect(window.locator('.board__status').last()).toHaveText('On hold')

  // The board card is a token card: its corner is --radius-control.
  expect(await computed(window, '.ticket-row--board', 'border-radius')).toBe(
    token('radius-control'),
  )

  // A board of one or two lanes does not stretch: a status column caps at the
  // card's readable width, so the one-lane home reads like the five-lane picture.
  const cell = await window.locator('.board__cell').first().boundingBox()
  expect(cell?.width ?? 0).toBeLessThanOrEqual(320)

  const boardIds = await rowIds(window)
  expect(boardIds.length).toBe(listIds.length)
  expect([...boardIds].sort()).toEqual([...listIds].sort())

  // The pictures are the Open board, the face's default.
  await window.getByRole('tab', { name: /^Open/ }).click()
  await capture(app, window, scene('tickets-board'), 1080)

  await window.getByRole('tab', { name: 'List' }).click()
  await expect(window.locator('.list')).toBeVisible()
  await expect(window.locator('.list__lane-title').first()).toBeVisible()
  await capture(app, window, scene('tickets-list'))
})

test('a drag moves a ticket across its lane and its status, and the move survives a restart', async ({
  app,
  window,
}) => {
  // Both axes of the grid in one window: the drop test needs a destination cell
  // whose lane and status both differ from the source's.
  await app.evaluate(({ BrowserWindow }) => {
    BrowserWindow.getAllWindows()[0]?.setContentSize(1400, 560)
  })
  await openTab(window, 'tickets/board')
  await expect(window.locator('.board__cell').first()).toBeVisible()
  await expect(window.locator('.ticket-row').first()).toBeVisible()

  const cells = window.locator('.board__cell')
  const count = await cells.count()
  expect(count).toBeGreaterThan(1)

  // The first cell that holds a card.
  let sourceIndex = -1
  for (let index = 0; index < count; index += 1) {
    if ((await cells.nth(index).locator('.ticket-row').count()) > 0) {
      sourceIndex = index
      break
    }
  }
  expect(sourceIndex).toBeGreaterThanOrEqual(0)

  const source = cells.nth(sourceIndex)
  const card = source.locator('.ticket-row').first()
  const ticket = await card.getAttribute('data-ticket')
  expect(ticket).not.toBeNull()
  const sourceLane = await source.getAttribute('data-lane')
  const sourceStatus = await source.getAttribute('data-status')

  // A cell in another lane and another status, so the drop makes both writes,
  // the lane first and the status after it.
  const target = window
    .locator(
      `.board__cell:not([data-lane="${sourceLane}"]):not([data-status="${sourceStatus}"])`,
    )
    .first()
  const targetLane = await target.getAttribute('data-lane')
  const targetStatus = await target.getAttribute('data-status')

  // Drop near the target cell's top edge, which is inside the viewport even
  // when the row is taller than the window.
  await card.dragTo(target, { targetPosition: { x: 40, y: 12 } })

  const landed = `.board__cell[data-lane="${targetLane}"][data-status="${targetStatus}"] .ticket-row[data-ticket="${ticket}"]`
  await expect(window.locator(landed)).toBeVisible()

  // A restart is the same read: the app holds no board state of its own.
  await window.reload()
  await window.waitForSelector('.shell')
  await openTab(window, 'tickets/board')
  await expect(window.locator(landed)).toBeVisible()
})

test('the keyboard set reaches the fields and moves a ticket, and a second writer lands live', async ({
  app,
  window,
}) => {
  await openTab(window, 'tickets/board')
  await expect(window.locator('.board__cell').first()).toBeVisible()
  await expect(window.locator('.ticket-row').first()).toBeVisible()

  // Quick-add and search, on the platform's modifier.
  await window.keyboard.press(`${MOD}+n`)
  await expect(window.locator('.quick-add__input')).toBeFocused()
  await window.keyboard.press(`${MOD}+f`)
  await expect(window.locator('.search__input')).toBeFocused()
  const firstTitle = (await window.locator('.ticket-row__title').first().innerText()).split(' ')[0]
  await window.keyboard.type(firstTitle ?? '')
  await expect(window.locator('.ticket-row').first()).toBeVisible()
  await window.keyboard.press('Escape')
  await expect(window.locator('.search__input')).toHaveValue('')

  // Leave the field: the arrows belong to the face again.
  await window.locator('.face__title').click()
  await window.keyboard.press('ArrowDown')
  const selected = window.locator('.ticket-row[aria-current="true"]')
  await expect(selected).toHaveCount(1)
  const ticket = await selected.getAttribute('data-ticket')
  expect(ticket).not.toBeNull()

  const before = await cellOf(window, ticket as string)
  const statuses = ['backlog', 'ready', 'in-progress', 'in-review', 'done', 'on-hold']
  const atStatus = statuses.indexOf(before.status ?? '')
  expect(atStatus).toBeGreaterThanOrEqual(0)
  // Step away from whichever end the ticket sits at, so there is a cell to land in.
  const step = atStatus >= statuses.length - 2 ? -1 : 1
  const statusAfter = statuses[atStatus + step]
  expect(statusAfter).not.toBeUndefined()

  // Option and the horizontal arrows move the selected ticket one status over,
  // inside its own lane: the status is the column.
  await window.keyboard.press(`Alt+Arrow${step < 0 ? 'Left' : 'Right'}`)
  await expect(
    window.locator(
      `.board__cell[data-lane="${before.lane}"][data-status="${statusAfter}"] .ticket-row[data-ticket="${ticket}"]`,
    ),
  ).toBeVisible()

  await capture(app, window, scene('tickets-keyboard'), 1080)

  // A second writer, the CLI in a terminal: the face updates with no reload.
  const title = 'Written from the CLI while the face was open'
  cli(['ticket', 'add', '--title', title])
  await expect(window.locator('.ticket-row', { hasText: title })).toBeVisible({ timeout: 15_000 })
})

test('the sheet opens inside the palette ring, not the OS accent, in both schemes', async ({
  window,
}) => {
  await openTab(window, 'tickets')
  await expect(window.locator('.ticket-row').first()).toBeVisible()

  // A sheet takes focus when it opens, so its ring is the ring drawn round the
  // whole dialog: it must come from the palette, never the browser's default.
  await window.locator('.ticket-row').first().dblclick()
  await expect(window.locator('.sheet')).toBeVisible()

  expect(await computed(window, '.sheet', 'outline-color')).toBe(token('accent'))

  // The night scheme overrides --accent, so the same rule must follow it.
  await window.emulateMedia({ colorScheme: 'dark' })
  expect(await computed(window, '.sheet', 'outline-color')).toBe(token('accent', 'night'))
  await window.emulateMedia({ colorScheme: 'light' })
})
