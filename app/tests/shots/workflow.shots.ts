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

/** The same store the app reads, through the CLI. */
function cliJson(args: string[]): unknown {
  return JSON.parse(
    execFileSync(binary(), args, {
      env: { ...process.env, TANDEM_HOME: seedPath(), TANDEM_AGENT: 'shots' },
      encoding: 'utf8',
    }),
  )
}

type ListedTicket = { id: string; status: string }

/**
 * The Workflow face a human reads: the lane picker and lifecycle controls, the
 * open sprint's header with its stored mode, the pass DAG with the batch on
 * its nodes by ticket status, and the run log. Every node and chip is the
 * wire's own over the fixture home.
 */
test('the workflow face draws the open sprint on the pass DAG', async ({ app, window }) => {
  // A lane with open tickets, opened through the CLI: the face draws the
  // wire's answers, so the run opens the record the way a human would, then
  // draws one batch ticket per status present so every node can carry one.
  const lanes = (cliJson(['lane', 'list']) as { lanes: string[] }).lanes
  let lane: string | null = null
  let batch: ListedTicket[] = []
  for (const name of lanes) {
    const listed = cliJson(['ticket', 'list', '--lane', name]) as { tickets: ListedTicket[] }
    if (listed.tickets.length > 0) {
      lane = name
      batch = listed.tickets
      break
    }
  }
  expect(lane).not.toBeNull()
  cli(['sprint', 'open', '--lane', lane as string, '--mode', 'parallel'])
  const drawn = new Set<string>()
  for (const ticket of batch) {
    if (drawn.has(ticket.status)) continue
    drawn.add(ticket.status)
    cli(['sprint', 'ticket', '--lane', lane as string, '--ticket', ticket.id])
  }

  await openTab(window, 'workflow')

  await expect(window.locator('.face__title')).toHaveText('Workflow')
  await expect(window.locator('.workflow__heading').first()).toContainText('Sprint 1')
  await expect(window.locator('.workflow__heading').first()).toContainText(lane as string)
  await expect(window.locator('.workflow__mode')).toHaveText('parallel')

  // The five passes, joined by their four handoffs.
  await expect(window.locator('.workflow__node')).toHaveCount(5)
  await expect(window.locator('.workflow__edge')).toHaveCount(4)

  // The nodes are the theme's card, the handoffs the accent.
  expect(await computed(window, '.workflow__node', 'fill')).toBe(token('parchment'))
  expect(await computed(window, '.workflow__edge', 'stroke')).toBe(token('accent-deep'))

  // Every drawn batch ticket sits on a node, named with its ref and title.
  const chips = window.locator('.workflow__chip')
  await expect(chips).toHaveCount(drawn.size)
  await expect(chips.first()).toContainText(':')

  // The run log is empty on a fresh open, and says so in the face's words.
  await expect(window.locator('.workflow__fact')).toContainText('No runs yet')

  // A lane with no open sprint draws the empty state with the mode opener.
  const others = lanes.filter((name) => name !== lane)
  if (others.length > 0) {
    await window.locator('.workflow__select').selectOption(others[0] as string)
    await expect(window.locator('.empty__title')).toContainText('No sprint is open')
    await expect(window.locator('.workflow__opener')).toBeVisible()
    await window.locator('.workflow__select').selectOption(lane as string)
    await expect(window.locator('.workflow__heading').first()).toContainText('Sprint 1')
  }

  await capture(app, window, scene('workflow'))
})
