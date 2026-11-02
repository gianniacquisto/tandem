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

test('the focused tickets face draws only the active lane plus the focus control', async ({
  app,
  window,
}) => {
  // One lane with open tickets, read through the CLI: whatever home the run
  // was told to read, the face must draw that lane alone.
  const lanes = (cliJson(['lane', 'list']) as { lanes: string[] }).lanes
  let focus: string | null = null
  for (const lane of lanes) {
    const listed = cliJson(['ticket', 'list', '--lane', lane]) as { tickets: unknown[] }
    if (listed.tickets.length > 0) {
      focus = lane
      break
    }
  }
  expect(focus).not.toBeNull()
  cli(['settings', 'set', '--project', focus as string])

  await openTab(window, 'tickets/board')
  await expect(window.locator('.board__cell').first()).toBeVisible()

  // The focus control names the stored focus...
  const picker = window.locator('.focus__picker')
  await expect(picker).toBeVisible()
  await expect(picker).toHaveValue(focus as string)

  // ...and the board draws that lane row alone.
  const rows = window.locator('.board__lane:not(.board__lane--head)')
  await expect(rows).toHaveCount(1)
  await expect(rows.first().locator('.board__title').first()).toContainText(focus as string)

  // The list draws the same lane alone.
  await window.getByRole('tab', { name: 'List' }).click()
  await expect(window.locator('.list__lane-title')).toHaveCount(1)
  await expect(window.locator('.list__lane-title').first()).toContainText(focus as string)

  await window.getByRole('tab', { name: 'Board' }).click()

  // The picture is the focused face: one lane row plus the focus control.
  await capture(app, window, scene('tickets-focus'), 1330)

  // The show-all escape draws every lane without clearing the stored focus.
  await window.getByRole('button', { name: 'Show all' }).click()
  await expect(window.getByRole('button', { name: 'Show focus' })).toHaveAttribute(
    'aria-pressed',
    'true',
  )
  await expect(picker).toHaveValue(focus as string)
  await window.getByRole('button', { name: 'Show focus' }).click()
  await expect(window.locator('.board__lane:not(.board__lane--head)')).toHaveCount(1)
})
