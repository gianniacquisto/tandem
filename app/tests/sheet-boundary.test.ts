// @vitest-environment happy-dom
/// <reference lib="dom" />
// The mount pulls the sheet into this (node) program, so it reads the
// renderer's window surface declaration too.
/// <reference path="../renderer/src/env.d.ts" />
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { createElement, type ReactElement, type ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { TicketSheet } from '../renderer/src/faces/TicketSheet'
import { SheetBoundary } from '../renderer/src/faces/SheetBoundary'
import type { CommentView } from '../renderer/src/wire/CommentView'
import type { TicketDetail } from '../renderer/src/wire/TicketDetail'

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

// happy-dom rewrites import.meta.url, so the fixtures resolve off the working
// directory the suite runs from (the app package) instead.
const fixtures = join(process.cwd(), 'tests', 'fixtures', 'tan312')
function fixture(name: string): string {
  return readFileSync(join(fixtures, name), 'utf8')
}

const ticket = JSON.parse(fixture('ticket.json')) as TicketDetail
const comments = JSON.parse(fixture('comments.json')) as CommentView[]
const descriptionBlocks = JSON.parse(fixture('blocks-desc.json'))
const commentBlocks = new Map<string, unknown>()
for (const [index, comment] of comments.entries()) {
  try {
    commentBlocks.set(
      comment.body,
      JSON.parse(fixture(`blocks-comment-${index}.json`)),
    )
  } catch {
    commentBlocks.set(comment.body, [])
  }
}

function paragraphOf(text: string): { blocks: unknown[] } {
  return { blocks: [{ kind: 'paragraph', runs: [{ text, styles: [], link: null }] }] }
}

function stubBackend(override?: (method: string, params: unknown) => unknown): void {
  const tandem = {
    call: async (message: { method: string; params: unknown }): Promise<unknown> => {
      if (override !== undefined) {
        const answer = override(message.method, message.params)
        if (answer !== undefined) return answer
      }
      const params = message.params as { text?: string | null; id?: string }
      switch (message.method) {
        case 'ticket.show':
          return { reply: { ticket, revision: 7 } }
        case 'ticket.comment.list':
          return { reply: { comments, revision: 7 } }
        case 'markdown.blocks':
          if (params.text === null || params.text === undefined) return { reply: { blocks: [] } }
          if (params.text === ticket.description) return { reply: { blocks: descriptionBlocks } }
          return { reply: { blocks: commentBlocks.get(params.text) ?? paragraphOf(params.text).blocks } }
        case 'session.list':
          return { reply: { sessions: [], revision: 7 } }
        default:
          throw new Error(`unexpected call ${message.method}`)
      }
    },
    onChanged: (): (() => void) => () => undefined,
  }
  Object.defineProperty(window, 'tandem', { value: tandem, configurable: true })
  try {
    Object.defineProperty(window.navigator, 'platform', { value: 'MacIntel', configurable: true })
  } catch {
    // happy-dom already names a platform: the hint reads it as-is.
  }
}

async function mountSheet(node: React.ReactElement): Promise<{ host: HTMLElement }> {
  const host = document.createElement('div')
  document.body.appendChild(host)
  await act(async () => {
    createRoot(host).render(node)
    await new Promise((resolve) => setTimeout(resolve, 0))
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
  return { host }
}

function sheetProps(extra?: Partial<Parameters<typeof TicketSheet>[0]>): Parameters<typeof TicketSheet>[0] {
  return {
    ticketId: ticket.id,
    expandedSession: null,
    lanes: ['Tandem', 'Retro'],
    roster: ['builder', 'human'],
    revision: 7,
    onWrite: async () => ({ ok: true }),
    onDismiss: vi.fn(),
    onOpenTicket: () => undefined,
    ...extra,
  }
}

describe('the sheet boundary (TAN-312)', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.restoreAllMocks()
  })

  it('draws its children when nothing throws', async () => {
    const { host } = await mountSheet(
      createElement(SheetBoundary, { onDismiss: () => undefined }, createElement('p', null, 'steady')),
    )
    expect(host.innerHTML).toContain('steady')
    expect(host.querySelector('[role="alert"]')).toBeNull()
    host.remove()
  })

  it('shows an inline error with a dismiss when a child throws, never a blank window', async () => {
    function Boom(): React.ReactNode {
      throw new Error('the sheet fell over')
    }
    const onDismiss = vi.fn()
    const errors: unknown[] = []
    const failing = console.error
    console.error = (...args: unknown[]): void => {
      errors.push(args)
    }
    try {
      const { host } = await mountSheet(
        createElement(SheetBoundary, { onDismiss }, createElement(Boom)),
      )
      try {
        expect(host.innerHTML).not.toBe('')
        const alert = host.querySelector('[role="alert"]')
        expect(alert).not.toBeNull()
        expect(alert?.textContent).toContain('the sheet fell over')
        const dismiss = host.querySelector('.sheet__actions button')
        expect(dismiss?.textContent).toBe('Dismiss')
        dismiss?.dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
        expect(onDismiss).toHaveBeenCalledTimes(1)
      } finally {
        host.remove()
      }
    } finally {
      console.error = failing
    }
  })

  it('opens TAN-306 from the real store payloads with no blank screen', async () => {
    stubBackend()
    const errors: unknown[] = []
    const failing = console.error
    console.error = (...args: unknown[]): void => {
      errors.push(args)
    }
    try {
      const { host } = await mountSheet(createElement(TicketSheet, sheetProps()))
      try {
        expect(host.innerHTML).toContain('Tandem needs epic cycle lanes')
        // The 1977-char brief draws its first heading through the markdown seam.
        expect(host.innerHTML).toContain('>Why<')
        // All ten log rows draw, through the same seam.
        expect(host.innerHTML).toContain('sprint')
        expect(host.querySelector('[role="alert"]')).toBeNull()
      } finally {
        host.remove()
      }
    } finally {
      console.error = failing
    }
    expect(errors).toEqual([])
  })

  it('opens a terse ticket with no description for writing, with no blank screen', async () => {
    stubBackend((method) => {
      if (method === 'ticket.show') {
        return { reply: { ticket: { ...ticket, id: 'terse', description: null, title: 'A terse ticket' }, revision: 7 } }
      }
      return undefined
    })
    const { host } = await mountSheet(
      createElement(TicketSheet, sheetProps({ ticketId: 'terse' })),
    )
    try {
      // A terse ticket opens in source view, ready for its first body.
      expect(host.innerHTML).toContain('A terse ticket')
      expect(host.innerHTML).toContain('sheet__source')
      expect(host.querySelector('[role="alert"]')).toBeNull()
    } finally {
      host.remove()
    }
  })

  it('draws an unknown markdown kind as a fallback inside the sheet, never a blank window', async () => {
    stubBackend((method, params) => {
      if (method === 'markdown.blocks') {
        const text = (params as { text?: string | null }).text
        if (text === ticket.description) {
          return {
            reply: {
              blocks: [
                ...(descriptionBlocks as unknown[]),
                { kind: 'fancy', text: 'a future core kind' },
              ],
            },
          }
        }
      }
      return undefined
    })
    const errors: unknown[] = []
    const failing = console.error
    console.error = (...args: unknown[]): void => {
      errors.push(args)
    }
    try {
      const { host } = await mountSheet(
        createElement(
          SheetBoundary,
          { onDismiss: () => undefined },
          createElement(TicketSheet, sheetProps()),
        ),
      )
      try {
        // The sheet stays up, the unknown block reads as a fallback, and the
        // boundary never fired: no inline error, no blank window.
        expect(host.innerHTML).toContain('Tandem needs epic cycle lanes')
        expect(host.innerHTML).toContain('cannot draw')
        expect(host.querySelector('.sheet__error')).toBeNull()
      } finally {
        host.remove()
      }
    } finally {
      console.error = failing
    }
    expect(errors).toEqual([])
  })

  it('opens a ticket whose edges and log rows are missing, with no blank screen', async () => {
    stubBackend((method) => {
      if (method === 'ticket.show') {
        return {
          reply: {
            ticket: { ...ticket, blockedBy: null, blocks: null, description: undefined },
            revision: 7,
          },
        }
      }
      if (method === 'ticket.comment.list') {
        return {
          reply: {
            comments: [
              { at: null, author: null, body: null },
              null,
              { at: '2026-11-04T15:39:55Z', author: 'sprint', body: 'Steady.' },
            ],
            revision: 7,
          },
        }
      }
      return undefined
    })
    const errors: unknown[] = []
    const failing = console.error
    console.error = (...args: unknown[]): void => {
      errors.push(args)
    }
    try {
      const { host } = await mountSheet(createElement(TicketSheet, sheetProps()))
      try {
        expect(host.innerHTML).toContain('Tandem needs epic cycle lanes')
        expect(host.innerHTML).toContain('Steady.')
        expect(host.innerHTML).toContain('anonymous')
        expect(host.querySelector('.sheet__error')).toBeNull()
      } finally {
        host.remove()
      }
    } finally {
      console.error = failing
    }
    expect(errors).toEqual([])
  })
})
