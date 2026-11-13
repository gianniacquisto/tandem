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
    lanes: ['Tandem', 'Retro'],
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

/** The sheet's three metadata controls, by the label the human reads. */
function control(host: HTMLElement, label: string): HTMLInputElement | HTMLSelectElement {
  const found = host.querySelector<HTMLInputElement | HTMLSelectElement>(
    `[aria-label="${label}"]`,
  )
  if (found === null) throw new Error(`the sheet draws no ${label} control`)
  return found
}

/** A pick through the control's own event, as a keyboard or a click makes it. */
function pick(host: HTMLElement, label: string, value: string): void {
  const element = control(host, label)
  // The native setter, so React's tracked value sees the change: assigning
  // `element.value` directly leaves the tracker holding the old value.
  const prototype =
    element instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype
  Object.getOwnPropertyDescriptor(prototype, 'value')?.set?.call(element, value)
  element.dispatchEvent(new window.Event('input', { bubbles: true }))
  element.dispatchEvent(new window.Event('change', { bubbles: true }))
}

async function press(host: HTMLElement, selector: string): Promise<void> {
  const button = host.querySelector<HTMLButtonElement>(selector)
  if (button === null) throw new Error(`the sheet draws no ${selector}`)
  await act(async () => {
    button.dispatchEvent(new window.MouseEvent('click', { bubbles: true }))
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
}

describe('the sheet metadata row (UI-34)', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.restoreAllMocks()
  })

  it('draws status, assignee and lane above the description, in the sticky band', async () => {
    stubBackend()
    const { host } = await mountSheet(createElement(TicketSheet, sheetProps()))
    try {
      const row = host.querySelector('.sheet__meta')
      expect(row).not.toBeNull()
      // The three facts, in the row and out of the collapsed Details.
      expect(row?.querySelector('select[aria-label="Status"]')).not.toBeNull()
      expect(row?.querySelector('input[aria-label="Assignee"]')).not.toBeNull()
      expect(row?.querySelector('select[aria-label="Lane"]')).not.toBeNull()
      // The status dot rides beside its select, so the colour reads with it.
      expect(row?.querySelector('.sheet__status-dot')).not.toBeNull()

      // The row is inside the sticky band, and the band is drawn above the
      // description in document order (4 is DOCUMENT_POSITION_FOLLOWING).
      const sticky = host.querySelector('.sheet__sticky')
      expect(sticky?.querySelector('.sheet__meta')).not.toBeNull()
      const description = host.querySelector('.sheet__description')
      expect(description).not.toBeNull()
      expect(Number(row?.compareDocumentPosition(description as Node) ?? 0) & 4).toBe(4)
    } finally {
      host.remove()
    }
  })

  it('says in sight which write each control makes', async () => {
    stubBackend()
    const { host } = await mountSheet(createElement(TicketSheet, sheetProps()))
    try {
      const row = host.querySelector('.sheet__meta')
      expect(row?.textContent).toContain('saves with Save')
      expect(row?.textContent).toContain('moves at once')
    } finally {
      host.remove()
    }
  })

  it('still starts its Details closed, with the row in reach', async () => {
    stubBackend()
    const { host } = await mountSheet(createElement(TicketSheet, sheetProps()))
    try {
      const details = Array.from(host.querySelectorAll<HTMLButtonElement>('button')).find(
        (button) => button.textContent === 'Details',
      )
      expect(details?.getAttribute('aria-expanded')).toBe('false')
      expect(host.querySelector('.sheet__details-body')).toBeNull()
      expect(control(host, 'Lane').closest('.sheet__details')).toBeNull()
    } finally {
      host.remove()
    }
  })

  it('opens a terse ticket with the row ready to write', async () => {
    stubBackend((method) => {
      if (method === 'ticket.show') {
        return {
          reply: { ticket: { ...ticket, id: 'terse', description: null, title: 'A terse ticket' }, revision: 7 },
        }
      }
      return undefined
    })
    const { host } = await mountSheet(
      createElement(TicketSheet, sheetProps({ ticketId: 'terse' })),
    )
    try {
      expect(control(host, 'Status').value).toBe(ticket.status)
      expect(control(host, 'Assignee').value).toBe(ticket.assignee)
      expect(control(host, 'Lane').value).toBe(ticket.lane)
      expect(host.querySelector('.sheet__meta')).not.toBeNull()
    } finally {
      host.remove()
    }
  })

  it('drafts status and assignee for one Save that carries only what moved', async () => {
    stubBackend()
    const onWrite = vi.fn(async () => ({ ok: true as const }))
    const { host } = await mountSheet(createElement(TicketSheet, sheetProps({ onWrite })))
    try {
      await act(async () => {
        pick(host, 'Status', 'in-progress')
        pick(host, 'Assignee', 'human')
      })
      // Neither pick writes: both wait for Save.
      expect(onWrite).not.toHaveBeenCalled()

      await press(host, '.sheet__save')
      expect(onWrite).toHaveBeenCalledTimes(1)
      expect(onWrite).toHaveBeenCalledWith(
        {
          method: 'ticket.set',
          params: {
            id: ticket.id,
            title: null,
            status: 'in-progress',
            assignee: 'human',
            description: null,
          },
        },
        7,
        undefined,
      )
    } finally {
      host.remove()
    }
  })

  it('moves the lane at once, and leaves Save nothing to write', async () => {
    stubBackend()
    const onWrite = vi.fn(async () => ({ ok: true as const }))
    const { host } = await mountSheet(createElement(TicketSheet, sheetProps({ onWrite })))
    try {
      await act(async () => {
        pick(host, 'Lane', 'Retro')
      })
      expect(onWrite).toHaveBeenCalledTimes(1)
      expect(onWrite).toHaveBeenCalledWith(
        { method: 'ticket.move', params: { id: ticket.id, lane: 'Retro', index: null } },
        7,
        undefined,
      )

      await press(host, '.sheet__save')
      expect(onWrite).toHaveBeenCalledTimes(1)
    } finally {
      host.remove()
    }
  })
})
