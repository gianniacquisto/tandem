import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import {
  INITIAL_FILTERS,
  appendIndex,
  assigneeChoices,
  assigneeLabel,
  blockLabel,
  dropIndex,
  filterTickets,
  laneColumns,
  laneIndexOf,
  laneKeys,
  laneMembers,
  neighbourLane,
  neighbourSelection,
  scopeCounts,
  visibleMoveIndex,
  statusLabel,
  titleMatches,
  type Filters,
} from '../renderer/src/shell/board'
import type { TicketSummary } from '../renderer/src/wire/TicketSummary'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const renderer = join(appRoot, 'renderer', 'src')

function source(name: string): string {
  return readFileSync(join(renderer, name), 'utf8')
}

function ticket(overrides: Partial<TicketSummary>): TicketSummary {
  return {
    archived: false,
    assignee: null,
    blocked: false,
    blockedBy: [],
    blocks: [],
    completedAt: null,
    createdAt: '2026-10-30T08:00:00Z',
    descriptionLength: 0,
    done: false,
    id: 'id',
    lane: null,
    link: null,
    number: null,
    origin: 'human',
    ref: null,
    status: 'backlog',
    title: 'a ticket',
    ...overrides,
  }
}

/** Four tickets over two lanes, in the store's order. */
const board: TicketSummary[] = [
  ticket({ id: 'a', title: 'Alpha', lane: 'Port', number: 1, ref: 'PRT-1', status: 'ready' }),
  ticket({ id: 'b', title: 'Beta', lane: null, number: null }),
  ticket({ id: 'c', title: 'Gamma', lane: 'Port', number: 2, ref: 'PRT-2', done: true, status: 'done' }),
  ticket({
    id: 'd',
    title: 'Delta',
    lane: 'Tandem',
    number: 3,
    ref: 'TAN-1',
    archived: true,
    done: true,
    status: 'done',
  }),
]

const filters = (overrides: Partial<Filters>): Filters => ({ ...INITIAL_FILTERS, ...overrides })

describe('the board view over the snapshot', () => {
  it('starts on the open board, every axis absent and no search', () => {
    expect(INITIAL_FILTERS).toEqual({
      scope: 'open',
      status: null,
      assignee: null,
      lane: null,
      text: '',
    })
  })

  it('counts the scopes the way the segmented control draws them', () => {
    expect(scopeCounts(board)).toEqual({ open: 2, done: 1, all: 4 })
  })

  it('parts open, done and all on the archive the way the app does', () => {
    expect(filterTickets(board, filters({ scope: 'open' })).map((t) => t.id)).toEqual(['a', 'b'])
    expect(filterTickets(board, filters({ scope: 'done' })).map((t) => t.id)).toEqual(['c'])
    expect(filterTickets(board, filters({ scope: 'all' })).map((t) => t.id)).toEqual([
      'a',
      'b',
      'c',
      'd',
    ])
  })

  it('narrows by status, assignee and lane, with the empty value naming the empty field', () => {
    expect(filterTickets(board, filters({ scope: 'all', status: 'ready' })).map((t) => t.id)).toEqual([
      'a',
    ])
    expect(filterTickets(board, filters({ scope: 'all', assignee: '' })).map((t) => t.id)).toEqual([
      'a',
      'b',
      'c',
      'd',
    ])
    expect(filterTickets(board, filters({ scope: 'all', lane: '' })).map((t) => t.id)).toEqual(['b'])
    expect(filterTickets(board, filters({ scope: 'all', lane: 'Port' })).map((t) => t.id)).toEqual([
      'a',
      'c',
    ])
  })

  it('searches the title case- and accent-insensitively, and never reorders', () => {
    const accented = [ticket({ id: 'x', title: 'Crème brûlée' }), ticket({ id: 'y', title: 'Creme' })]

    expect(titleMatches('creme', 'Crème brûlée')).toBe(true)
    expect(titleMatches('CREME', 'creme')).toBe(true)
    expect(filterTickets(accented, filters({ text: '  crème ' })).map((t) => t.id)).toEqual([
      'x',
      'y',
    ])
    expect(filterTickets(accented, filters({ text: 'brulee' })).map((t) => t.id)).toEqual(['x'])
    expect(filterTickets(board, filters({ scope: 'all', text: 'a' })).map((t) => t.id)).toEqual([
      'a',
      'b',
      'c',
      'd',
    ])
  })

  it('groups by lane in render order: the default area, declared lanes, then dangling names', () => {
    const keys = laneKeys(['Port'], board)
    expect(keys).toEqual([null, 'Port', 'Tandem'])

    const columns = laneColumns(['Port'], board)
    expect(columns.map((column) => column.title)).toEqual(['Uncategorized', 'Port', 'Tandem'])
    expect(columns.map((column) => column.tickets.map((t) => t.id))).toEqual([
      ['b'],
      ['a', 'c'],
      ['d'],
    ])
  })

  it('draws a declared lane with no tickets, so it stays a drop target', () => {
    const columns = laneColumns(['Port', 'Empty'], board)
    expect(columns.map((column) => column.title)).toEqual(['Uncategorized', 'Port', 'Empty', 'Tandem'])
    expect(columns[2]?.tickets).toEqual([])
  })

  it('titles the default area, and keeps a dangling lane rather than hiding its tickets', () => {
    const keys = laneKeys([], [ticket({ id: 'z', lane: 'Hand edit' })])
    expect(keys).toEqual([null, 'Hand edit'])
    expect(laneColumns([], [ticket({ id: 'z', lane: 'Hand edit' })])[1]?.title).toBe('Hand edit')
  })

  it('labels a status the way the domain spells it, and an assignee the way every face does', () => {
    expect(statusLabel('backlog')).toBe('Backlog')
    expect(statusLabel('on-hold')).toBe('On hold')
    expect(statusLabel('in-review')).toBe('In Review')
    expect(assigneeLabel(null)).toBeNull()
    expect(assigneeLabel('human')).toBe('human')
    expect(assigneeLabel('builder')).toBe('@builder')
  })

  it('marks a block by the one open blocker it can name, or plainly', () => {
    expect(blockLabel(ticket({ blocked: false }))).toBeNull()
    expect(blockLabel(ticket({ blocked: true }))).toBe('Blocked')
    expect(
      blockLabel(
        ticket({
          blocked: true,
          blockedBy: [{ id: 'x', ref: 'PRT-12', status: 'in-progress', title: 'the shell' }],
        }),
      ),
    ).toBe('Blocked, PRT-12')
    expect(
      blockLabel(
        ticket({
          blocked: true,
          blockedBy: [
            { id: 'x', ref: 'PRT-12', status: 'in-progress', title: 'the shell' },
            { id: 'y', ref: 'PRT-14', status: 'ready', title: 'the bikeshed' },
          ],
        }),
      ),
    ).toBe('Blocked')
  })
})

describe('the drag arithmetic', () => {
  it('lands before or after the anchor it was dropped on', () => {
    // Drop `a` on `c`'s bottom half: after c, inside Port.
    expect(dropIndex(board, 'a', 'c', true)).toBe(1)
    // Drop `a` on `c`'s top half: before c.
    expect(dropIndex(board, 'a', 'c', false)).toBe(0)
  })

  it('subtracts one when the moving ticket already sat before the drop point', () => {
    // c is after a; dropping a on c's bottom half after removal is index 1.
    expect(dropIndex(board, 'c', 'a', true)).toBe(1)
  })

  it('answers the anchor lane, even when the moving ticket came from another lane', () => {
    expect(dropIndex(board, 'b', 'c', true)).toBe(2)
  })

  it('ignores a drop on itself, and answers nothing for an unknown anchor', () => {
    expect(dropIndex(board, 'a', 'a', true)).toBeNull()
    expect(dropIndex(board, 'a', 'missing', true)).toBeNull()
    // The moving id is the drag's own payload, so the core resolves only the
    // anchor; an id that is not in the lane simply subtracts nothing.
    expect(dropIndex(board, 'missing', 'a', true)).toBe(1)
  })

  it('appends at the end of a lane, and finds a ticket and its lane by id', () => {
    expect(appendIndex(board, 'Port')).toBe(2)
    expect(appendIndex(board, 'Rien')).toBe(0)
    expect(laneIndexOf(board, 'c')).toBe(1)
    expect(laneIndexOf(board, 'missing')).toBeNull()
    expect(laneMembers(board, 'Port').map((t) => t.id)).toEqual(['a', 'c'])
  })

  it('moves one visible row at a time, stepping over the rows a filter hides', () => {
    const full = [
      ticket({ id: 'a', lane: 'Port' }),
      ticket({ id: 'b', lane: 'Port' }),
      ticket({ id: 'c', lane: 'Port' }),
    ]
    // b is hidden: moving a down lands it after c, not after b.
    const visible = [full[0], full[2]] as TicketSummary[]
    expect(visibleMoveIndex(full, visible, 'a', 1)).toBe(2)
    expect(visibleMoveIndex(full, visible, 'c', -1)).toBe(0)
    expect(visibleMoveIndex(full, visible, 'a', -1)).toBeNull()
    expect(visibleMoveIndex(full, visible, 'c', 1)).toBeNull()
    expect(visibleMoveIndex(full, visible, 'b', 1)).toBeNull()
  })

  it('walks the lane order, where null is a real key and undefined is past the end', () => {
    const keys = laneKeys(['Port'], board)
    expect(neighbourLane(keys, 'Port', 1)).toBe('Tandem')
    expect(neighbourLane(keys, 'Port', -1)).toBeNull()
    expect(neighbourLane(keys, null, 1)).toBe('Port')
    expect(neighbourLane(keys, 'Tandem', 1)).toBeUndefined()
  })

  it('is the shared golden both implementations run (Fixtures/port/ordering.json)', () => {
    type Case = {
      name: string
      tickets: Array<{ id: string; lane: string | null }>
      moving: string
      anchor: string
      after: boolean
      index: number | null
    }
    const golden = JSON.parse(
      readFileSync(fileURLToPath(new URL('../../Fixtures/port/ordering.json', import.meta.url)), 'utf8'),
    ) as { cases: Case[] }

    expect(golden.cases.length).toBeGreaterThan(0)
    for (const entry of golden.cases) {
      const tickets = entry.tickets.map((one) => ticket({ id: one.id, lane: one.lane }))
      expect(dropIndex(tickets, entry.moving, entry.anchor, entry.after), entry.name).toBe(entry.index)
    }
  })

  it('moves the selection down a column, then across to the next non-empty lane', () => {
    const columns = laneColumns(['Port', 'Empty'], board)
    expect(columns.map((column) => column.title)).toEqual(['Uncategorized', 'Port', 'Empty', 'Tandem'])

    expect(neighbourSelection(columns, null, 1)).toBe('b')
    // Across from the default area skips the empty lane to Port's first row.
    expect(neighbourSelection(columns, 'b', 1)).toBe('a')
    expect(neighbourSelection(columns, 'a', 1)).toBe('d')
    expect(neighbourSelection(columns, 'd', 1)).toBe('d')
    expect(neighbourSelection(columns, 'd', -1)).toBe('a')
  })
})

describe('the assignee lists', () => {
  // The Swift order (`AssigneePicker.options`, PRT-35): Unassigned, the reserved
  // word, the roster, then the ticket's own name when the roster has dropped it.
  it('offers the picker in the Swift order: unassigned, human, roster, off-roster current', () => {
    expect(assigneeChoices(['builder', 'human', ' review '], 'review')).toEqual([
      { value: null, label: 'Unassigned', selected: false },
      { value: 'human', label: 'human', selected: false },
      { value: 'builder', label: '@builder', selected: false },
      { value: 'review', label: '@review', selected: true },
    ])
    expect(assigneeChoices([], null)[0]?.selected).toBe(true)
  })

  it('offers a stored roster name that no ticket carries', () => {
    // PRT-38: the names come from the store's `roster` setting, so a name with
    // no row is still a name the human can assign.
    const values = assigneeChoices(['human', 'review'], null).map((choice) => choice.value)
    expect(values).toEqual([null, 'human', 'review'])
  })

  // The rule is one helper, not one per face: the sheet's picker and the
  // board's filter draw the same names from the same list, so a name the sheet
  // can assign is a name the filter can narrow to.
  it('draws both assignee lists from the store roster, not from the board rows', () => {
    for (const name of ['faces/TicketSheet.tsx', 'faces/TicketsFace.tsx']) {
      expect(source(name), `${name} uses the one helper`).toContain('assigneeChoices(')
    }
    for (const name of ['App.tsx', 'faces/TicketsFace.tsx']) {
      expect(source(name), `${name} derives no name from a ticket row`).not.toContain(
        'ticket.assignee',
      )
    }
  })
})
