import { execFileSync } from 'node:child_process'
import { join } from 'node:path'

import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { seedPath } from '../../shots/home'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'

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

/** The board column a card sits in, by its lane key (the default area is the empty string). */
async function laneOf(
  window: import('@playwright/test').Page,
  ticket: string,
): Promise<string | null> {
  return window
    .locator('.board__column', {
      has: window.locator(`.ticket-row[data-ticket="${ticket}"]`),
    })
    .first()
    .getAttribute('data-lane')
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

  // The same rows, laid out in lane columns: same tickets, same order, cut the
  // other way.
  await window.getByRole('tab', { name: 'Board' }).click()
  await expect(window.locator('.board')).toBeVisible()
  await expect(window.locator('.board__column').first()).toBeVisible()

  // A board of one or two lanes does not stretch: a column caps at the card's
  // readable width, so the one-lane home reads like the five-lane picture.
  const column = await window.locator('.board__column').first().boundingBox()
  expect(column?.width ?? 0).toBeLessThanOrEqual(320)

  const boardIds = await rowIds(window)
  expect(boardIds).toEqual(listIds)

  // The pictures are the Open board, the face's default.
  await window.getByRole('tab', { name: /^Open/ }).click()
  await capture(app, window, scene('tickets-board'), 1080)

  await window.getByRole('tab', { name: 'List' }).click()
  await expect(window.locator('.list')).toBeVisible()
  await expect(window.locator('.list__lane-title').first()).toBeVisible()
  await capture(app, window, scene('tickets-list'))
})

test('a drag moves a ticket to another lane, and the move survives a restart', async ({
  app,
  window,
}) => {
  await openTab(window, 'tickets/board')
  await expect(window.locator('.board__column').first()).toBeVisible()
  await expect(window.locator('.ticket-row').first()).toBeVisible()

  const columns = window.locator('.board__column')
  const count = await columns.count()
  expect(count).toBeGreaterThan(1)

  // The first card of the first column that holds one.
  let sourceColumn = -1
  for (let index = 0; index < count; index += 1) {
    if ((await columns.nth(index).locator('.ticket-row').count()) > 0) {
      sourceColumn = index
      break
    }
  }
  expect(sourceColumn).toBeGreaterThanOrEqual(0)

  const card = columns.nth(sourceColumn).locator('.ticket-row').first()
  const ticket = await card.getAttribute('data-ticket')
  expect(ticket).not.toBeNull()

  const targetColumn = sourceColumn === 0 ? 1 : sourceColumn - 1
  const target = columns.nth(targetColumn)
  const targetLane = await target.getAttribute('data-lane')

  await card.dragTo(target.locator('.board__title'))

  await expect(
    window.locator(
      `.board__column[data-lane="${targetLane}"] .ticket-row[data-ticket="${ticket}"]`,
    ),
  ).toBeVisible()

  // A restart is the same read: the app holds no board state of its own.
  await window.reload()
  await window.waitForSelector('.shell')
  await openTab(window, 'tickets/board')
  await expect(
    window.locator(
      `.board__column[data-lane="${targetLane}"] .ticket-row[data-ticket="${ticket}"]`,
    ),
  ).toBeVisible()
})

test('the keyboard set reaches the fields and moves a ticket, and a second writer lands live', async ({
  app,
  window,
}) => {
  await openTab(window, 'tickets/board')
  await expect(window.locator('.board__column').first()).toBeVisible()
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

  const laneBefore = await laneOf(window, ticket as string)

  // A vertical Option arrow moves one visible row; a row a filter hides is
  // stepped over rather than counted. On a one-row lane there is nowhere to go,
  // so the horizontal move below is the exercise.
  const inLane = window.locator(`.board__column[data-lane="${laneBefore}"] .ticket-row`)
  const visibleInLane = await Promise.all(
    (await inLane.all()).map((row) => row.getAttribute('data-ticket')),
  )
  const atInLane = visibleInLane.indexOf(ticket as string)
  if (atInLane >= 0 && atInLane < visibleInLane.length - 1) {
    await window.keyboard.press('Alt+ArrowDown')
    await expect(
      window.locator(`.board__column[data-lane="${laneBefore}"] .ticket-row`).nth(atInLane + 1),
    ).toHaveAttribute('data-ticket', ticket as string)
  }

  // Option and the arrows move the selected ticket one lane over.
  const lanes = await Promise.all(
    (await window.locator('.board__column').all()).map((node) => node.getAttribute('data-lane')),
  )
  const at = lanes.indexOf(laneBefore)
  const step = at > 0 ? -1 : 1
  const laneAfter = lanes[at + step]
  expect(laneAfter).not.toBeUndefined()

  await window.keyboard.press(`Alt+Arrow${step < 0 ? 'Left' : 'Right'}`)
  await expect(
    window.locator(`.board__column[data-lane="${laneAfter}"] .ticket-row[data-ticket="${ticket}"]`),
  ).toBeVisible()

  await capture(app, window, scene('tickets-keyboard'), 1080)

  // A second writer, the CLI in a terminal: the face updates with no reload.
  const title = 'Written from the CLI while the face was open'
  cli(['ticket', 'add', '--title', title])
  await expect(window.locator('.ticket-row', { hasText: title })).toBeVisible({ timeout: 15_000 })
})
