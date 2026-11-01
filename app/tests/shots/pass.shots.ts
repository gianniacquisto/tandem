import { execFileSync } from 'node:child_process'
import { join } from 'node:path'

import { expect } from '@playwright/test'

import { capture, openTab, test } from '../../shots/app'
import { seedPath } from '../../shots/home'
import { REPO_ROOT } from '../../shots/roots'
import { scene } from '../../shots/scenes'
import { computed, token } from '../../shots/tokens'

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

test('the sheet asks for passes as requests, with each disabled reason in place', async ({
  app,
  window,
}) => {
  // A fresh backlog ticket, so the sheet opens on refine enabled and the other
  // two disabled with their reasons.
  const title = 'A backlog ticket asking for its refine pass'
  cli(['ticket', 'add', '--title', title])

  await openTab(window, 'tickets')
  const row = window.locator('.ticket-row', { hasText: title })
  await expect(row.first()).toBeVisible()
  await row.first().dblclick()

  const sheet = window.locator('.sheet')
  await expect(sheet).toBeVisible()

  await expect(sheet.getByRole('button', { name: 'Request refine' })).toBeEnabled()
  await expect(sheet.getByRole('button', { name: 'Request build' })).toBeDisabled()
  await expect(sheet.getByRole('button', { name: 'Request review' })).toBeDisabled()
  await expect(sheet.locator('.sheet__pass-reason')).toHaveCount(2)

  // The reason is dim ink, never body ink: it explains, it does not shout.
  expect(await computed(window, '.sheet__pass-reason', 'color')).toBe(token('ink-dim'))

  await capture(app, window, scene('ticket-sheet-passes'))
})
