import { describe, expect, it, vi } from 'vitest'

import {
  PASS_COMMENT,
  passButtons,
  requestPass,
  type PassTicket,
} from '../renderer/src/shell/pass'

function ticket(overrides: Partial<PassTicket> = {}): PassTicket {
  return {
    id: 'ticket-1',
    status: 'backlog',
    blocked: false,
    assignee: null,
    ...overrides,
  }
}

function buttonFor(
  buttons: ReturnType<typeof passButtons>,
  kind: 'refine' | 'build' | 'review',
) {
  const found = buttons.find((button) => button.kind === kind)
  if (found === undefined) throw new Error(`no ${kind} button`)
  return found
}

describe('the pass-request buttons', () => {
  it('offers refine on an unblocked backlog ticket, and nothing else', () => {
    const buttons = passButtons(ticket({ status: 'backlog' }), [])

    expect(buttonFor(buttons, 'refine').enabled).toBe(true)
    expect(buttonFor(buttons, 'refine').reason).toBeNull()
    expect(buttonFor(buttons, 'build').enabled).toBe(false)
    expect(buttonFor(buttons, 'review').enabled).toBe(false)
  })

  it('offers build on an unblocked ready ticket, and nothing else', () => {
    const buttons = passButtons(ticket({ status: 'ready' }), [])

    expect(buttonFor(buttons, 'build').enabled).toBe(true)
    expect(buttonFor(buttons, 'build').reason).toBeNull()
    expect(buttonFor(buttons, 'refine').enabled).toBe(false)
    expect(buttonFor(buttons, 'review').enabled).toBe(false)
  })

  it('offers review on an in-review ticket, and nothing else', () => {
    const buttons = passButtons(ticket({ status: 'in-review' }), [])

    expect(buttonFor(buttons, 'review').enabled).toBe(true)
    expect(buttonFor(buttons, 'review').reason).toBeNull()
    expect(buttonFor(buttons, 'refine').enabled).toBe(false)
    expect(buttonFor(buttons, 'build').enabled).toBe(false)
  })

  it('offers no button on a blocked ticket, and says it is blocked', () => {
    const buttons = passButtons(ticket({ status: 'backlog', blocked: true }), [])

    for (const kind of ['refine', 'build', 'review'] as const) {
      const button = buttonFor(buttons, kind)
      expect(button.enabled).toBe(false)
      expect(button.reason).toMatch(/blocked/i)
    }
  })

  it('names the blocked reason before the wrong status', () => {
    const buttons = passButtons(ticket({ status: 'done', blocked: true }), [])

    expect(buttonFor(buttons, 'build').reason).toMatch(/blocked/i)
  })

  it('says why a wrong-status ticket gets no button', () => {
    const buttons = passButtons(ticket({ status: 'done' }), [])

    for (const kind of ['refine', 'build', 'review'] as const) {
      const button = buttonFor(buttons, kind)
      expect(button.enabled).toBe(false)
      expect(button.reason).not.toBeNull()
    }
    expect(buttonFor(buttons, 'refine').reason).toMatch(/backlog/i)
    expect(buttonFor(buttons, 'build').reason).toMatch(/ready/i)
    expect(buttonFor(buttons, 'review').reason).toMatch(/in-review/i)
  })

  it('treats an assignee of that role as already claimed', () => {
    const refine = passButtons(ticket({ status: 'backlog', assignee: 'refiner' }), [])
    expect(buttonFor(refine, 'refine').enabled).toBe(false)
    expect(buttonFor(refine, 'refine').reason).toMatch(/refiner/i)

    const build = passButtons(ticket({ status: 'ready', assignee: 'builder' }), [])
    expect(buttonFor(build, 'build').enabled).toBe(false)
    expect(buttonFor(build, 'build').reason).toMatch(/builder/i)
  })

  it('treats its own request comment as already requested', () => {
    const refine = passButtons(ticket({ status: 'backlog' }), [PASS_COMMENT.refine])
    expect(buttonFor(refine, 'refine').enabled).toBe(false)
    expect(buttonFor(refine, 'refine').reason).toMatch(/already requested/i)

    const build = passButtons(ticket({ status: 'ready' }), [PASS_COMMENT.build])
    expect(buttonFor(build, 'build').enabled).toBe(false)

    const review = passButtons(ticket({ status: 'in-review' }), [PASS_COMMENT.review])
    expect(buttonFor(review, 'review').enabled).toBe(false)
    expect(buttonFor(review, 'review').reason).toMatch(/already requested/i)
  })

  it('ignores other passes’ request comments', () => {
    const buttons = passButtons(ticket({ status: 'backlog' }), [PASS_COMMENT.build])

    expect(buttonFor(buttons, 'refine').enabled).toBe(true)
  })
})

describe('the pass-request writes', () => {
  it('asks for refine with the claim the refiner obeys, plus the request comment', () => {
    const buttons = passButtons(ticket({ id: 't1', status: 'backlog' }), [])

    expect(buttonFor(buttons, 'refine').calls).toEqual([
      {
        method: 'ticket.set',
        params: { id: 't1', title: null, status: null, assignee: 'refiner', description: null },
      },
      {
        method: 'ticket.comment.add',
        params: { id: 't1', author: 'human', body: PASS_COMMENT.refine },
      },
    ])
  })

  it('asks for build with the claim the builder takes, plus the request comment', () => {
    const buttons = passButtons(ticket({ id: 't1', status: 'ready' }), [])

    expect(buttonFor(buttons, 'build').calls).toEqual([
      {
        method: 'ticket.set',
        params: {
          id: 't1',
          title: null,
          status: 'in-progress',
          assignee: 'builder',
          description: null,
        },
      },
      {
        method: 'ticket.comment.add',
        params: { id: 't1', author: 'human', body: PASS_COMMENT.build },
      },
    ])
  })

  it('asks for review with a comment only: status and assignee untouched', () => {
    const buttons = passButtons(ticket({ id: 't1', status: 'in-review' }), [])

    expect(buttonFor(buttons, 'review').calls).toEqual([
      {
        method: 'ticket.comment.add',
        params: { id: 't1', author: 'human', body: PASS_COMMENT.review },
      },
    ])
  })

  it('reuses only methods the wire declares, so no UI-only write appears', () => {
    for (const status of ['backlog', 'ready', 'in-review']) {
      for (const button of passButtons(ticket({ status }), [])) {
        for (const call of button.calls) {
          expect(call.method === 'ticket.set' || call.method === 'ticket.comment.add').toBe(true)
        }
      }
    }
  })
})

describe('the pass-request path', () => {
  it('sends refine’s two frames in order through the bridge call', async () => {
    const seen: unknown[] = []
    const call = vi.fn(async (frame: unknown) => {
      seen.push(frame)
      return { ok: true }
    })

    await expect(requestPass(call, 't1', 'refine')).resolves.toBe(true)

    expect(seen).toEqual([
      {
        method: 'ticket.set',
        params: { id: 't1', title: null, status: null, assignee: 'refiner', description: null },
      },
      {
        method: 'ticket.comment.add',
        params: { id: 't1', author: 'human', body: PASS_COMMENT.refine },
      },
    ])
    expect(call).toHaveBeenCalledTimes(2)
  })

  it('sends review as one comment frame', async () => {
    const seen: unknown[] = []
    const call = vi.fn(async (frame: unknown) => {
      seen.push(frame)
      return { ok: true }
    })

    await expect(requestPass(call, 't1', 'review')).resolves.toBe(true)

    expect(seen).toEqual([
      {
        method: 'ticket.comment.add',
        params: { id: 't1', author: 'human', body: PASS_COMMENT.review },
      },
    ])
  })

  it('stops at the first refused write and reports the failure', async () => {
    const seen: unknown[] = []
    const call = vi.fn(async (frame: unknown) => {
      seen.push(frame)
      return { ok: false }
    })

    await expect(requestPass(call, 't1', 'build')).resolves.toBe(false)
    expect(seen).toHaveLength(1)
  })
})
