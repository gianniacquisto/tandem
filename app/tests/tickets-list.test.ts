import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { boardStatuses } from '../renderer/src/shell/board'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const css = readFileSync(`${appRoot}renderer/src/app.css`, 'utf8')
const tokens = readFileSync(`${appRoot}renderer/src/tokens.css`, 'utf8')
const row = readFileSync(`${appRoot}renderer/src/faces/TicketRow.tsx`, 'utf8')
const face = readFileSync(`${appRoot}renderer/src/faces/TicketsFace.tsx`, 'utf8')

/** The relative luminance of a hex colour, per WCAG. */
function luminance(hex: string): number {
  const value = parseInt(hex.slice(1), 16)
  const channels = [(value >> 16) & 255, (value >> 8) & 255, value & 255].map((part) => {
    const channel = part / 255
    return channel <= 0.03928 ? channel / 12.92 : Math.pow((channel + 0.055) / 1.055, 2.4)
  })
  return 0.2126 * (channels[0] ?? 0) + 0.7152 * (channels[1] ?? 0) + 0.0722 * (channels[2] ?? 0)
}

function contrast(first: string, second: string): number {
  const high = Math.max(luminance(first), luminance(second))
  const low = Math.min(luminance(first), luminance(second))
  return (high + 0.05) / (low + 0.05)
}

function token(name: string): string {
  const block = tokens.match(/^:root\s*\{([\s\S]*?)^\}/m)?.[1] ?? ''
  const value = block.match(new RegExp(`--${name}:\\s*([^;]+);`))?.[1]?.trim()
  if (value === undefined) throw new Error(`no --${name} in the day :root block`)
  return value
}

/** One rule's body for a literal selector, or null when the rule is absent. */
function rule(selector: string): string | null {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return css.match(new RegExp(`${escaped}\\s*\\{([\\s\\S]*?)\\n\\}`, 'm'))?.[1] ?? null
}

describe('the tickets list hierarchy and state colour (TAND-11)', () => {
  it('reads the quick-add as the primary action', () => {
    const quickAdd = rule('.quick-add')
    expect(quickAdd, '.quick-add keeps a filled control').toContain('background: var(--parchment)')

    const focused = rule('.quick-add:focus-within')
    expect(focused, 'focus stays in the palette accent, not the ember error').toContain(
      'border-color: var(--accent)',
    )

    const plus = rule('.quick-add__plus')
    expect(plus, 'the plus carries the accent, not the quiet text').toContain(
      'color: var(--accent)',
    )
  })

  it('keeps the search quiet until it is in use', () => {
    const search = rule('.search')
    expect(search, 'the search carries no fill until focused').toContain('background: transparent')
    expect(search, 'the search carries no loud border until focused').toMatch(/border:\s*0;/)
    expect(search, 'the search rests as an icon, not a full-width row').toContain('width: 34px')

    expect(
      css,
      'the search opens on focus or on a draft, then reads as the secondary field',
    ).toContain('.search:has(.search__input:not(:placeholder-shown))')
  })

  it('demotes the filters to text with a chevron so the counts read first', () => {
    const filter = rule('.filter')
    expect(filter, 'a filter is text, not a boxed control').toMatch(/border:\s*0;/)
    expect(filter, 'a filter sits on no fill').toContain('background: transparent')
  })

  it('draws the search beside the scopes and filters, not on its own row', () => {
    const barStart = face.indexOf('<div className="tickets__bar">')
    const searchAt = face.indexOf('<div className="search">')
    const viewsAt = face.indexOf('segmented--views')
    expect(barStart, 'the face keeps one toolbar row').toBeGreaterThanOrEqual(0)
    expect(searchAt, 'the face keeps the search').toBeGreaterThanOrEqual(0)
    expect(searchAt, 'the search sits inside the toolbar row').toBeGreaterThan(barStart)
    expect(searchAt, 'the search sits beside the scopes, before the layout toggle').toBeLessThan(
      viewsAt,
    )
  })

  it('gives blocked, on-hold, ready and in-review a colour before the label is read', () => {
    expect(row, 'the row exposes its status for the stylesheet').toContain('data-status')
    for (const status of ['on-hold', 'ready', 'in-progress', 'in-review']) {
      const statusRule = rule(`.ticket-row__status[data-status="${status}"]`)
      expect(statusRule, `${status} names its own chip colour`).toContain('--chip:')
    }
    const chips = new Set(
      ['on-hold', 'ready', 'in-progress', 'in-review'].map(
        (status) =>
          rule(`.ticket-row__status[data-status="${status}"]`)?.match(/--chip:\s*([^;]+);/)?.[1]?.trim(),
      ),
    )
    expect(chips.size, 'every state but backlog carries its own hue').toBe(4)
    expect(chips.has('var(--ink-dim)'), 'no state reuses the neutral chip').toBe(false)

    expect(css, 'the blocked mark stays warm').toContain('.ticket-row__tag--blocked')
  })

  it('keeps backlog neutral: no pill when there is nothing to say', () => {
    expect(row, 'backlog draws no status pill').toContain("ticket.status === 'backlog' ? null")
  })

  it('meets AA on the small text the list reads in quiet ink', () => {
    expect(contrast(token('ink-dim'), token('paper')), 'quiet ink on paper').toBeGreaterThanOrEqual(
      4.5,
    )
    expect(
      contrast(token('ink-dim'), token('parchment')),
      'quiet ink on the hover fill',
    ).toBeGreaterThanOrEqual(4.5)
  })

  it('draws the status filter from the fixed six statuses', () => {
    expect(face, 'the filter options come from the fixed status list').toContain(
      'const statuses = boardStatuses()',
    )
    expect(face, 'the status filter maps over those statuses').toMatch(
      /\{statuses\.map\(\(status\) => \(/,
    )
  })

  it('the_list_status_filter_carries_the_six_fixed_statuses', () => {
    expect(
      boardStatuses().map((status) => status.value),
      'the six statuses in order',
    ).toEqual(['backlog', 'on-hold', 'ready', 'in-progress', 'in-review', 'done'])
    const ready = boardStatuses().find((status) => status.value === 'ready')
    expect(ready?.label).toBe('Ready')
  })

  it('gives every non-backlog status its own pill rule', () => {
    // Backlog draws no pill; the other five carry a colour rule each.
    expect(row).toContain("ticket.status === 'backlog' ? null")
    expect(row).toContain('statusLabel(ticket.status)')
    for (const status of ['on-hold', 'ready', 'in-progress', 'in-review', 'done']) {
      expect(rule(`.ticket-row__status[data-status="${status}"]`), status).not.toBeNull()
    }
    expect(css).toContain('background: var(--chip, var(--ink-dim))')
  })

  it('leaves the board layout alone: this pass is list rows only', () => {
    expect(
      css,
      'no board card lane hiding rides along with the list pass',
    ).not.toContain('.ticket-row--board .ticket-row__lane')
  })
})
