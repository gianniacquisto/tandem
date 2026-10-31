import { describe, expect, it } from 'vitest'

import {
  INITIAL_FILTERS,
  appendIndex,
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
  statusLabel,
  titleMatches,
  type Filters,
} from '../renderer/src/shell/board'
import type { TicketSummary } from '../renderer/src/wire/TicketSummary'

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

  it('walks the lane order, where null is a real key and undefined is past the end', () => {
    const keys = laneKeys(['Port'], board)
    expect(neighbourLane(keys, 'Port', 1)).toBe('Tandem')
    expect(neighbourLane(keys, 'Port', -1)).toBeNull()
    expect(neighbourLane(keys, null, 1)).toBe('Port')
    expect(neighbourLane(keys, 'Tandem', 1)).toBeUndefined()
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
