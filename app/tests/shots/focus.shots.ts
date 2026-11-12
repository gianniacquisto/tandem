import { execFileSync } from 'node:child_process'
import { join } from 'node:path'

import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { seedPath } from '../../shots/home'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'

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

/** The same store the app reads, through the CLI. */
function cliJson(args: string[]): unknown {
  return JSON.parse(
    execFileSync(binary(), args, {
      env: { ...process.env, TANDEM_HOME: seedPath(), TANDEM_AGENT: 'shots' },
      encoding: 'utf8',
    }),
  )
}

test('the focused tickets face draws only the active project across the columns', async ({
  app,
  window,
}) => {
  // One project holding tickets, read through the CLI: whatever home the run
  // was told to read, the face must draw that project alone.
  const projects = (
    cliJson(['project', 'list']) as { projects: Array<{ name: string; lanes: string[] }> }
  ).projects
  let focus: { name: string; lanes: string[] } | null = null
  let focusCount = 0
  for (const project of projects) {
    let count = 0
    for (const lane of project.lanes) {
      const listed = cliJson(['ticket', 'list', '--filter', 'all', '--lane', lane]) as {
        tickets: unknown[]
      }
      count += listed.tickets.length
    }
    if (count > 0) {
      focus = project
      focusCount = count
      break
    }
  }
  expect(focus).not.toBeNull()
  cli(['settings', 'set', '--project', (focus as { name: string }).name])

  await openTab(window, 'tickets/board')
  await expect(window.locator('.board__column').first()).toBeVisible()

  // The focus control names the stored project...
  const picker = window.locator('.focus__picker')
  await expect(picker).toBeVisible()
  await expect(picker).toHaveValue((focus as { name: string }).name)

  // ...and the board draws that project's cards across the seven columns: every
  // lane chip names one of the project's lanes, and the cards are the store's.
  const project = focus as { name: string; lanes: string[] }
  const chips = await window.locator('.ticket-card__lane').allInnerTexts()
  expect(chips.length).toBe(focusCount)
  for (const chip of chips) expect(project.lanes).toContain(chip.trim())
  await expect(window.locator('.board__column')).toHaveCount(7)

  // The list draws the same project alone, at its All scope so a done project
  // still draws its lanes.
  await window.getByRole('tab', { name: 'List' }).click()
  await window.getByRole('tab', { name: /^All/ }).click()
  const titles = await window.locator('.list__lane-title > span:first-child').allInnerTexts()
  expect(titles.length).toBeGreaterThan(0)
  for (const title of titles) expect(project.lanes).toContain(title)

  await window.getByRole('tab', { name: 'Board' }).click()

  // The picture is the focused face: one project across the seven columns, plus
  // the focus control.
  await capture(app, window, scene('tickets-focus'), 1330)

  // The show-all escape draws every project without clearing the stored focus.
  await window.getByRole('button', { name: 'Show all' }).click()
  await expect(window.getByRole('button', { name: 'Show focus' })).toHaveAttribute(
    'aria-pressed',
    'true',
  )
  await expect(picker).toHaveValue(project.name)
  const live = cliJson(['ticket', 'list', '--filter', 'all']) as { tickets: unknown[] }
  await expect(window.locator('.ticket-card')).toHaveCount(live.tickets.length)
  await window.getByRole('button', { name: 'Show focus' }).click()
  await expect(window.locator('.ticket-card')).toHaveCount(focusCount)
})
