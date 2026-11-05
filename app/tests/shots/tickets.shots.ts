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

  await expect(window.locator('.face__title')).toHaveCount(0)
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

  // Projects own lanes: the default area first and ungrouped, then each
  // project header with its lanes beneath in lane order. Empty projects draw
  // no rows, so every header on the board names a project with lanes.
  const headers = window.locator('[data-project]')
  await expect(headers).not.toHaveCount(0)
  const firstLane = window.locator('.board__lane[data-lane]').first()
  await expect(firstLane).toHaveAttribute('data-lane', '')
  if ((process.env['TANDEM_SHOTS_HOME'] ?? '').endsWith('Fixtures/port/Board')) {
    await expect(headers).toHaveCount(4)
    await expect(headers.nth(0)).toHaveAttribute('data-project', 'Port')
    await expect(headers.nth(1)).toHaveAttribute('data-project', 'Tandem')
    await expect(headers.nth(2)).toHaveAttribute('data-project', 'Retro')
    await expect(headers.nth(3)).toHaveAttribute('data-project', 'Fleet')
  }

  // The pictures are the Open board, the face's default.
  await window.getByRole('tab', { name: /^Open/ }).click()
  // The grid needs 1288 px for its lane title and six status columns, and the
  // shell spends a gutter either side of it: a 1080 window scrolls On hold off
  // the right edge, and the picture would be half a board.
  await capture(app, window, scene('tickets-board'), 1330)
  // The frame just shot holds the whole board: a column past the edge is a
  // column the photograph never has.
  await expect(window.locator('.board__status').last()).toBeInViewport()

  await window.getByRole('tab', { name: 'List' }).click()
  await expect(window.locator('.list')).toBeVisible()
  await expect(window.locator('.list__lane-title').first()).toBeVisible()

  // The list reads as a centred column at the board capture's width: wider
  // than the old 540px face, narrower than the window, and every store
  // ticket still draws as a row.
  await app.evaluate(({ BrowserWindow }) => {
    BrowserWindow.getAllWindows()[0]?.setContentSize(1330, 560)
  })
  const listBox = await window.locator('.list').boundingBox()
  expect(listBox?.width ?? 0).toBeGreaterThan(540)
  expect(listBox?.width ?? 0).toBeLessThanOrEqual(820)
  // The picture is the Open scope, so the wide list must draw every Open
  // ticket: the count the face's own tab names, each one a store ticket.
  const openLabel = (await window.getByRole('tab', { name: /^Open/ }).innerText()).trim()
  const openCount = Number(openLabel.split(/\s+/).pop())
  const wideIds = await rowIds(window)
  expect(wideIds).toHaveLength(openCount)
  for (const id of wideIds) expect(storeIds).toContain(id)

  await capture(app, window, scene('tickets-list'), 1330)
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
  await window.locator('h2.board__title').first().click()
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

  await capture(app, window, scene('tickets-keyboard'), 1330)

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
  const sheet = window.locator('.sheet')
  await expect(sheet).toBeVisible()
  // Pin the sheet's focus before reading its ring: the dialog takes focus
  // only once its ticket read lands, so a read straight after visibility
  // races that write and sees the browser default instead of the palette.
  // Focusing here keeps the ring deterministic whatever the previous spec
  // left the window's focus on.
  await sheet.focus()
  await expect(sheet).toBeFocused()

  expect(await computed(window, '.sheet', 'outline-color')).toBe(token('accent'))

  // The night scheme overrides --accent, so the same rule must follow it.
  await window.emulateMedia({ colorScheme: 'dark' })
  expect(await computed(window, '.sheet', 'outline-color')).toBe(token('accent', 'night'))
  await window.emulateMedia({ colorScheme: 'light' })
})

test('a quick-add lands its row and opens its sheet in the description composer, and Esc returns', async ({
  window,
}) => {
  await openTab(window, 'tickets')
  await expect(window.locator('.ticket-row').first()).toBeVisible()

  const title = 'A quick-added ticket that opens its own sheet'
  await window.locator('.quick-add__input').fill(title)
  await window.locator('.quick-add__input').press('Enter')

  // The row lands via the change event; the sheet opens from the add receipt.
  const row = window.locator('.ticket-row', { hasText: title })
  await expect(row.first()).toBeVisible({ timeout: 15_000 })
  const sheet = window.locator('.sheet')
  await expect(sheet).toBeVisible()

  // Description draws before Comments before collapsed Details.
  await expect(sheet.locator('.sheet__description')).toBeVisible()
  await expect(sheet.locator('.sheet__discussion')).toBeVisible()
  const order = await window.evaluate(() => {
    const root = document.querySelector('.sheet')
    const description = root?.querySelector('.sheet__description')
    const comments = root?.querySelector('.sheet__discussion')
    const details = root?.querySelector('.sheet__details')
    if (description == null || comments == null || details == null) return null
    // 4 is DOCUMENT_POSITION_FOLLOWING: the node project carries no DOM lib,
    // so the constant is literal rather than window.Node.
    const descriptionBeforeComments = Boolean(description.compareDocumentPosition(comments) & 4)
    const commentsBeforeDetails = Boolean(comments.compareDocumentPosition(details) & 4)
    return { descriptionBeforeComments, commentsBeforeDetails }
  })
  expect(order).toEqual({ descriptionBeforeComments: true, commentsBeforeDetails: true })

  // Details starts collapsed on every opening and persists nothing.
  await expect(sheet.getByRole('button', { name: 'Details' })).toHaveAttribute(
    'aria-expanded',
    'false',
  )
  await expect(sheet.locator('.sheet__details-body')).toHaveCount(0)

  // The handoff lands in the description composer, ready for the first body.
  await expect(sheet.getByLabel('Add to the description')).toBeFocused()

  // Esc returns via the existing dismiss, back to the face that opened it.
  await window.keyboard.press('Escape')
  await expect(sheet).toHaveCount(0)
  await expect(window.locator('.ticket-row').first()).toBeVisible()
})
