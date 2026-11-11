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

async function cardIds(window: import('@playwright/test').Page): Promise<Array<string | null>> {
  const cards = await window.locator('.ticket-card').all()
  return Promise.all(cards.map((card) => card.getAttribute('data-ticket')))
}

/** The board column a card sits in, read from its section's data-status. */
async function columnOf(
  window: import('@playwright/test').Page,
  ticket: string,
): Promise<string | null> {
  const column = window
    .locator('.board__column', {
      has: window.locator(`.ticket-card[data-ticket="${ticket}"]`),
    })
    .first()
  return column.getAttribute('data-status')
}

/** One ticket as the store holds it, through the CLI. */
function storedTicket(id: string): { status: string; lane: string | null } {
  const reply = cliJson(['ticket', 'show', '--id', id]) as {
    status: string
    lane: string | null
  }
  return { status: reply.status, lane: reply.lane }
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
  const live = cliJson(['ticket', 'list', '--filter', 'all']) as { tickets: Array<{ id: string }> }
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

  // The board draws one column per workflow stage, in stage order, Done
  // included, and ignores the list's scope: its set is every non-archived
  // ticket the store holds.
  await window.getByRole('tab', { name: 'Board' }).click()
  await expect(window.locator('.board')).toBeVisible()
  await expect(window.locator('.board__column')).toHaveCount(6)
  expect(await window.locator('.board__column-label').allInnerTexts()).toEqual([
    'Backlog',
    'On hold',
    'Ready',
    'In progress',
    'In Review',
    'Done',
  ])
  // Each head's count is the number of cards in its column.
  for (const column of await window.locator('.board__column').all()) {
    const count = Number(await column.locator('.board__column-count').innerText())
    expect(await column.locator('.ticket-card').count()).toBe(count)
  }
  const boardIds = await cardIds(window)
  expect(boardIds.length).toBe(live.tickets.length)
  expect([...boardIds].sort()).toEqual(live.tickets.map((one) => one.id).sort())

  // The board card is a token card: its corner is --radius-control.
  expect(await computed(window, '.ticket-card', 'border-radius')).toBe(token('radius-control'))

  // A column caps at the card's readable width, so a one-project home reads
  // like the crowded one.
  const column = await window.locator('.board__column').first().boundingBox()
  expect(column?.width ?? 0).toBeLessThanOrEqual(360)

  // The pictures are the whole board: every stage, Done included, inside the
  // 1330 px capture width.
  await capture(app, window, scene('tickets-board'), 1330)
  await expect(window.locator('.board__column[data-status="done"]')).toBeInViewport()

  await window.getByRole('tab', { name: 'List' }).click()
  await expect(window.locator('.list')).toBeVisible()
  await expect(window.locator('.list__lane-title').first()).toBeVisible()

  // The pictures are the Open board, the list's own default.
  await window.getByRole('tab', { name: /^Open/ }).click()
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

test('a drag moves a ticket across columns and leaves its lane alone, and the move survives a restart', async ({
  app,
  window,
}) => {
  await app.evaluate(({ BrowserWindow }) => {
    BrowserWindow.getAllWindows()[0]?.setContentSize(1400, 560)
  })
  await openTab(window, 'tickets/board')
  await expect(window.locator('.board__column').first()).toBeVisible()
  await expect(window.locator('.ticket-card').first()).toBeVisible()

  const card = window.locator('.ticket-card').first()
  const ticket = await card.getAttribute('data-ticket')
  expect(ticket).not.toBeNull()
  const before = storedTicket(ticket as string)
  const sourceStatus = await columnOf(window, ticket as string)
  expect(sourceStatus).toBe(before.status)

  // A column with a different stage: the drop is one status write, no lane
  // write and no follow-up.
  const targetStatus = ['backlog', 'on-hold', 'ready', 'in-progress', 'in-review', 'done'].find(
    (status) => status !== sourceStatus,
  )
  expect(targetStatus).not.toBeUndefined()
  const target = window.locator(`.board__column[data-status="${targetStatus}"]`)

  await card.dragTo(target, { targetPosition: { x: 40, y: 12 } })

  const landed = `.board__column[data-status="${targetStatus}"] .ticket-card[data-ticket="${ticket}"]`
  await expect(window.locator(landed)).toBeVisible()

  const after = storedTicket(ticket as string)
  expect(after.status).toBe(targetStatus)
  expect(after.lane, 'the lane is a label, not an axis: a drop never writes it').toBe(before.lane)

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
  await expect(window.locator('.board__column').first()).toBeVisible()
  await expect(window.locator('.ticket-card').first()).toBeVisible()

  // The add shortcut reaches the first column's add, the search keeps its own.
  await window.keyboard.press(`${MOD}+n`)
  await expect(window.locator('.board__add-button').first()).toBeFocused()
  await window.keyboard.press(`${MOD}+f`)
  await expect(window.locator('.search__input')).toBeFocused()
  const firstTitle = (await window.locator('.ticket-card__title').first().innerText()).split(' ')[0]
  await window.keyboard.type(firstTitle ?? '')
  await expect(window.locator('.ticket-card').first()).toBeVisible()
  await window.keyboard.press('Escape')
  await expect(window.locator('.search__input')).toHaveValue('')

  // Leave the field: the arrows belong to the face again.
  await window.locator('.board__column-title').first().click()
  await window.keyboard.press('ArrowDown')
  const selected = window.locator('.ticket-card[aria-current="true"]')
  await expect(selected).toHaveCount(1)
  const ticket = await selected.getAttribute('data-ticket')
  expect(ticket).not.toBeNull()

  const beforeStatus = await columnOf(window, ticket as string)
  const statuses = ['backlog', 'on-hold', 'ready', 'in-progress', 'in-review', 'done']
  const atStatus = statuses.indexOf(beforeStatus ?? '')
  expect(atStatus).toBeGreaterThanOrEqual(0)
  // Step away from whichever end the ticket sits at, so there is a column to land in.
  const step = atStatus >= statuses.length - 2 ? -1 : 1
  const statusAfter = statuses[atStatus + step]
  expect(statusAfter).not.toBeUndefined()

  // Option and the horizontal arrows move the selected ticket one column over:
  // the status is the column, and the lane is left alone.
  await window.keyboard.press(`Alt+Arrow${step < 0 ? 'Left' : 'Right'}`)
  await expect(
    window.locator(
      `.board__column[data-status="${statusAfter}"] .ticket-card[data-ticket="${ticket}"]`,
    ),
  ).toBeVisible()

  await capture(app, window, scene('tickets-keyboard'), 1330)

  // A second writer, the CLI in a terminal: the face updates with no reload.
  const title = 'Written from the CLI while the face was open'
  cli(['ticket', 'add', '--title', title])
  await expect(window.locator('.ticket-card', { hasText: title })).toBeVisible({ timeout: 15_000 })
})

test('one click on a card opens the sheet inside the palette ring, in both schemes', async ({
  window,
}) => {
  await openTab(window, 'tickets/board')
  await expect(window.locator('.ticket-card').first()).toBeVisible()

  // A sheet takes focus when it opens, so its ring is the ring drawn round the
  // whole dialog: it must come from the palette, never the browser's default.
  await window.locator('.ticket-card').first().click()
  const sheet = window.locator('.sheet')
  await expect(sheet).toBeVisible()
  // Pin the sheet's focus before reading its ring: the dialog takes focus
  // only once its ticket read lands, so a read straight after visibility
  // races that write and sees the browser default instead of the palette.
  await sheet.focus()
  await expect(sheet).toBeFocused()

  expect(await computed(window, '.sheet', 'outline-color')).toBe(token('accent'))

  // The night scheme overrides --accent, so the same rule must follow it.
  await window.emulateMedia({ colorScheme: 'dark' })
  expect(await computed(window, '.sheet', 'outline-color')).toBe(token('accent', 'night'))
  await window.emulateMedia({ colorScheme: 'light' })
})

test("a card's sheet draws over the board's column heads, with the title and Save in sight, in both schemes", async ({
  app,
  window,
}) => {
  // The scene photographs the 78-ticket board home: the contract home's two
  // tickets hold no Ready card to open, so the run over it skips the scene.
  test.skip(
    !(process.env['TANDEM_SHOTS_HOME'] ?? '').endsWith('Fixtures/port/Board'),
    'the scene photographs the board home',
  )
  await openTab(window, 'tickets/board')
  await expect(window.locator('.ticket-card').first()).toBeVisible()

  // A Ready card: the dialog's sticky band then lands over the middle
  // columns' heads, the ones the bug painted over it.
  const card = window.locator('.ticket-card', { hasText: 'the ticket sheet and the lane surface' })
  await expect(card).toHaveCount(1)
  await card.click()

  const sheet = window.locator('.sheet')
  await expect(sheet).toBeVisible()
  // The dialog takes focus once its ticket read lands, so wait for it rather
  // than race that write with a read.
  await expect(sheet).toBeFocused()

  // The sticky band is the top surface in full: the title input answers its
  // own click, so no column head paints over it, and the Save sits beside it.
  const title = sheet.locator('.sheet__title')
  await expect(title).toBeVisible()
  await expect(title).toHaveValue('the ticket sheet and the lane surface')
  await title.click()
  await expect(title).toBeFocused()
  await expect(sheet.locator('.sheet__save')).toBeVisible()

  // The stacking the fix carries: the backdrop's level clears every sticky
  // and popover layer in the face, so the sheet paints above the column heads
  // whatever the scheme.
  const stacking = (): Promise<{ back: string; heads: string; above: boolean } | null> =>
    window.evaluate(() => {
      const backdrop = document.querySelector('.sheet-backdrop')
      const head = document.querySelector('.board__column-head')
      if (backdrop === null || head === null) return null
      const back = getComputedStyle(backdrop).zIndex
      const heads = getComputedStyle(head).zIndex
      return { back, heads, above: Number(back) > Number(heads) }
    })
  expect(await stacking()).toEqual({ back: '60', heads: '1', above: true })
  expect(await computed(window, '.sheet-backdrop', 'z-index')).toBe('60')

  await window.emulateMedia({ colorScheme: 'dark' })
  expect(await stacking()).toEqual({ back: '60', heads: '1', above: true })
  await window.emulateMedia({ colorScheme: 'light' })

  // The picture is the scene: the sheet over the whole board, the dialog's
  // own ring on it, the heads under the dim.
  await sheet.focus()
  await expect(sheet).toBeFocused()
  await capture(app, window, scene('tickets-board-sheet'), 1330)

  // Esc returns via the existing dismiss, back to the board that opened it.
  await window.keyboard.press('Escape')
  await expect(sheet).toHaveCount(0)
  await expect(card).toBeVisible()
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
