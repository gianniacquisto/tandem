import type { Call } from '../../../src/shared/bridge'

import { HUMAN_AUTHOR } from './sheet'

/**
 * The ticket sheet's pass requests: buttons that ask for the next agent pass,
 * never runs of it.
 *
 * A pass is a subagent dispatch from an agent session, but starting one takes
 * a host install and the CLI. The app already moves tickets through the same
 * verbs over the rpc wire, so each button writes what the next dispatch
 * already obeys: the assignee (plus the request comment) is the queue, and no
 * new queue is built. The sprint's triage loop stays the only runner, and
 * drawing the loop stays TAN-262's: this file acts on one ticket and draws
 * nothing.
 *
 * No new wire method, no new CLI verb: both writes reuse methods the CLI
 * already exposes (`ticket.set`, `ticket.comment.add`), so the no-UI-only-
 * writes contract holds with nothing new to prove.
 */

export type PassKind = 'refine' | 'build' | 'review'

/**
 * The words the request leaves in the ticket's log. One marker per pass, so a
 * sheet that reads the log can tell an asked pass from an unasked one, and the
 * next dispatch can tell an app request from a claim typed at a terminal.
 */
export const PASS_COMMENT: Record<PassKind, string> = {
  refine: 'Pass requested from the app: refine',
  build: 'Pass requested from the app: build',
  review: 'Pass requested from the app: review',
}

/** The ticket state a request reads: status, block, and who holds it. */
export type PassTicket = {
  id: string
  status: string
  blocked: boolean
  assignee: string | null
}

export type PassButton = {
  kind: PassKind
  /** The words on the button: a request, never an execution. */
  label: string
  enabled: boolean
  /** The reason drawn in place of the act when disabled; null when enabled. */
  reason: string | null
  /** The writes the request makes, in order. */
  calls: Call[]
}

function commentCall(id: string, kind: PassKind): Call {
  return {
    method: 'ticket.comment.add',
    params: { id, author: HUMAN_AUTHOR, body: PASS_COMMENT[kind] },
  }
}

function requested(comments: readonly string[], kind: PassKind): boolean {
  return comments.some((body) => body.includes(PASS_COMMENT[kind]))
}

function refineButton(ticket: PassTicket, comments: readonly string[]): PassButton {
  const base = { kind: 'refine' as const, label: 'Request refine' }
  if (ticket.blocked) {
    return { ...base, enabled: false, reason: 'Blocked: clear its blockers first.', calls: [] }
  }
  if (ticket.status !== 'backlog') {
    return { ...base, enabled: false, reason: 'Refine is for backlog tickets.', calls: [] }
  }
  if (ticket.assignee === 'refiner') {
    return { ...base, enabled: false, reason: 'Already assigned to the refiner.', calls: [] }
  }
  if (requested(comments, 'refine')) {
    return { ...base, enabled: false, reason: 'Refine already requested.', calls: [] }
  }
  return {
    ...base,
    enabled: true,
    reason: null,
    calls: [
      {
        method: 'ticket.set',
        params: { id: ticket.id, title: null, status: null, assignee: 'refiner', description: null },
      },
      commentCall(ticket.id, 'refine'),
    ],
  }
}

function buildButton(ticket: PassTicket, comments: readonly string[]): PassButton {
  const base = { kind: 'build' as const, label: 'Request build' }
  if (ticket.blocked) {
    return { ...base, enabled: false, reason: 'Blocked: clear its blockers first.', calls: [] }
  }
  if (ticket.status !== 'ready') {
    return { ...base, enabled: false, reason: 'Build is for ready tickets.', calls: [] }
  }
  if (ticket.assignee === 'builder') {
    return { ...base, enabled: false, reason: 'Already claimed by the builder.', calls: [] }
  }
  if (requested(comments, 'build')) {
    return { ...base, enabled: false, reason: 'Build already requested.', calls: [] }
  }
  // The request is the claim the build pass takes (`ticket set --status
  // in-progress --assignee builder`): with no agent session behind it the
  // store reads it as a claim, so the dead-claim and reclaim rules cover it
  // unchanged.
  return {
    ...base,
    enabled: true,
    reason: null,
    calls: [
      {
        method: 'ticket.set',
        params: {
          id: ticket.id,
          title: null,
          status: 'in-progress',
          assignee: 'builder',
          description: null,
        },
      },
      commentCall(ticket.id, 'build'),
    ],
  }
}

function reviewButton(ticket: PassTicket, comments: readonly string[]): PassButton {
  const base = { kind: 'review' as const, label: 'Request review' }
  if (ticket.blocked) {
    return { ...base, enabled: false, reason: 'Blocked: clear its blockers first.', calls: [] }
  }
  if (ticket.status !== 'in-review') {
    return { ...base, enabled: false, reason: 'Review is for in-review tickets.', calls: [] }
  }
  if (requested(comments, 'review')) {
    return { ...base, enabled: false, reason: 'Review already requested.', calls: [] }
  }
  // Comment only: the gate dispatch reads the PR from the log, never a fresh
  // claim, so status and assignee are untouched.
  return { ...base, enabled: true, reason: null, calls: [commentCall(ticket.id, 'review')] }
}

/**
 * The sheet's three pass buttons for one ticket: refine, build, review, each a
 * request with its disabled reason. `comments` are the bodies of the ticket's
 * log, the already-requested check.
 */
export function passButtons(
  ticket: PassTicket,
  comments: readonly string[],
): [PassButton, PassButton, PassButton] {
  return [refineButton(ticket, comments), buildButton(ticket, comments), reviewButton(ticket, comments)]
}

/** The status each pass asks from: the only state its button enables on. */
const PASS_STATUS: Record<PassKind, string> = {
  refine: 'backlog',
  build: 'ready',
  review: 'in-review',
}

/**
 * One request's writes through one call, in the button's order: the first
 * refusal stops the act and the request reports the failure, so a claim
 * without its comment (or a comment without its claim) is never half-told.
 */
export async function requestPass(
  call: (call: Call) => Promise<{ ok: boolean }>,
  ticketId: string,
  kind: PassKind,
): Promise<boolean> {
  const buttons = passButtons(
    { id: ticketId, status: PASS_STATUS[kind], blocked: false, assignee: null },
    [],
  )
  const button = buttons.find((entry) => entry.kind === kind)
  if (button === undefined) return false
  for (const next of button.calls) {
    const outcome = await call(next)
    if (!outcome.ok) return false
  }
  return true
}
