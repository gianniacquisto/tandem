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
  boardStatuses,
  dropCalls,
  dropIndex,
  emptyBoardCopy,
  filterTickets,
  firstSelection,
  laneColumns,
  laneIndexOf,
  laneKeys,
  laneMembers,
  laneRows,
  neighbourLane,
  neighbourSelection,
  scopeCounts,
  visibleLaneKeys,
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

  it('draws a row per lane and a cell per status, in the declared workflow order', () => {
    expect(boardStatuses().map((status) => status.value)).toEqual([
      'backlog',
      'on-hold',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])

    const rows = laneRows(['Port'], board)
    expect(rows.map((row) => row.title)).toEqual(['Uncategorized', 'Port', 'Tandem'])
    expect(rows[1]?.cells.map((cell) => cell.status)).toEqual([
      'backlog',
      'on-hold',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])
    expect(rows[1]?.cells.map((cell) => cell.tickets.map((t) => t.id))).toEqual([
      [],
      [],
      ['a'],
      [],
      [],
      ['c'],
    ])
  })

  it('draws every status cell of a declared lane, so each stays a drop target', () => {
    const rows = laneRows(['Port', 'Empty'], board)
    expect(rows.map((row) => row.title)).toEqual(['Uncategorized', 'Port', 'Empty', 'Tandem'])
    expect(rows[2]?.cells).toHaveLength(6)
    expect(rows[2]?.cells.every((cell) => cell.tickets.length === 0)).toBe(true)
  })

  it('selects the first card of the first lane that holds one, in either direction', () => {
    const rows = laneRows(['Port'], board)
    expect(firstSelection(rows, 1)).toBe('b')
    expect(firstSelection(rows, -1)).toBe('d')

    // A lane with no cards is stepped over, not answered with null.
    const sparse = laneRows([], [ticket({ id: 'z', lane: 'Port' })])
    expect(firstSelection(sparse, 1)).toBe('z')
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

describe('a drop on the board grid', () => {
  const methods = (calls: ReturnType<typeof dropCalls>) => calls.map((call) => call.method)

  it('issues ticket.set when only the status changes', () => {
    const calls = dropCalls(board, 'a', { lane: 'Port', status: 'in-review', index: 0 })

    expect(methods(calls)).toEqual(['ticket.set'])
    expect(calls[0]).toEqual({
      method: 'ticket.set',
      params: { id: 'a', title: null, status: 'in-review', assignee: null, description: null },
    })
  })

  it('issues ticket.move when only the lane changes', () => {
    const calls = dropCalls(board, 'a', { lane: 'Tandem', status: 'ready', index: 3 })

    expect(methods(calls)).toEqual(['ticket.move'])
    expect(calls[0]).toEqual({
      method: 'ticket.move',
      params: { id: 'a', lane: 'Tandem', index: 3 },
    })
  })

  it('issues ticket.move when neither changes, reordering inside the cell', () => {
    const calls = dropCalls(board, 'a', { lane: 'Port', status: 'ready', index: 1 })

    expect(methods(calls)).toEqual(['ticket.move'])
    expect(calls[0]).toEqual({
      method: 'ticket.move',
      params: { id: 'a', lane: 'Port', index: 1 },
    })
  })

  it('issues the lane write first, then the status write, when both change', () => {
    const calls = dropCalls(board, 'a', { lane: 'Tandem', status: 'done', index: 2 })

    expect(methods(calls)).toEqual(['ticket.move', 'ticket.set'])
    expect(calls[0]).toEqual({
      method: 'ticket.move',
      params: { id: 'a', lane: 'Tandem', index: 2 },
    })
    expect(calls[1]).toEqual({
      method: 'ticket.set',
      params: { id: 'a', title: null, status: 'done', assignee: null, description: null },
    })
  })

  it('issues nothing for a ticket the listing does not hold', () => {
    expect(dropCalls(board, 'missing', { lane: 'Port', status: 'ready', index: 0 })).toEqual([])
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

describe('the project focus', () => {
  const keys = laneKeys(['Port', 'Tandem'], board)

  it('draws the active lane alone', () => {
    expect(visibleLaneKeys(keys, 'Port')).toEqual(['Port'])
  })

  it('draws every lane key in render order when nothing is focused: the show-all escape', () => {
    expect(visibleLaneKeys(keys, null)).toEqual(keys)
    expect(visibleLaneKeys(keys, null)).toEqual([null, 'Port', 'Tandem'])
  })

  it('falls back to every lane when the focus names no lane key, so a stale focus hides nothing', () => {
    expect(visibleLaneKeys(keys, 'Retro')).toEqual(keys)
    expect(visibleLaneKeys(keys, '')).toEqual(keys)
  })
})

describe('the epic lane journey', () => {
  const epicDeclared = ['Tandem', 'ui', 'core'] as const
  const epicBoard: TicketSummary[] = [
    ticket({ id: 'e1', title: 'Triage inbox', lane: 'Tandem', status: 'backlog' }),
    ticket({ id: 'e2', title: 'Epic ui work', lane: 'ui', status: 'ready' }),
    ticket({ id: 'e3', title: 'Epic core work', lane: 'core', status: 'in-progress' }),
    ticket({ id: 'e4', title: 'Unfiled', lane: null, status: 'backlog' }),
  ]

  it('focuses an epic lane alone and shows all on the escape or a stale name', () => {
    const keys = laneKeys([...epicDeclared], epicBoard)
    expect(keys).toEqual([null, 'Tandem', 'ui', 'core'])
    expect(keys[0]).toBeNull()
    expect(visibleLaneKeys(keys, 'ui')).toEqual(['ui'])
    expect(visibleLaneKeys(keys, null)).toEqual(keys)
    expect(visibleLaneKeys(keys, 'docs')).toEqual(keys)
    expect(visibleLaneKeys(keys, '')).toEqual(keys)
  })

  it('narrows the list to the epic lane without reordering', () => {
    const ordered: TicketSummary[] = [
      ticket({ id: 'u1', title: 'First ui', lane: 'ui', status: 'ready' }),
      ticket({ id: 'c1', title: 'Core work', lane: 'core', status: 'ready' }),
      ticket({ id: 'u2', title: 'Second ui', lane: 'ui', status: 'in-progress' }),
      ticket({ id: 't1', title: 'Triage', lane: 'Tandem', status: 'backlog' }),
    ]
    const narrowed = filterTickets(ordered, filters({ scope: 'all', lane: 'ui' }))
    expect(narrowed.map((row) => row.id)).toEqual(['u1', 'u2'])
  })

  it('keeps an empty focused epic lane as a drop target with its hint', () => {
    const withoutUi = epicBoard.filter((row) => row.lane !== 'ui')
    const rows = laneRows([...epicDeclared], withoutUi)
    expect(rows.map((row) => row.key)).toEqual([null, 'Tandem', 'ui', 'core'])
    const uiRow = rows.find((row) => row.key === 'ui')
    expect(uiRow).toBeDefined()
    expect(uiRow?.cells).toHaveLength(6)
    expect(uiRow?.cells.every((cell) => cell.tickets.length === 0)).toBe(true)
    const cell = source('faces/BoardCell.tsx')
    expect(cell).toContain('board__drop-hint')
    expect(cell).toContain('Drop tickets here')
    expect(cell).toContain('data-empty')
    expect(cell).toContain('data-targeted')
    expect(
      emptyBoardCopy({
        status: 'ready',
        ticketCount: 2,
        error: null,
        filters: filters({}),
        nothingVisible: true,
        activeProject: 'ui',
        laneNames: [...epicDeclared],
      }),
    ).toMatchObject({ title: 'No tickets in ui' })
  })

  it('drops a ticket into an epic lane cell in planner order', () => {
    const calls = dropCalls(epicBoard, 'e1', { lane: 'ui', status: 'done', index: 1 })
    expect(calls.map((call) => call.method)).toEqual(['ticket.move', 'ticket.set'])
    expect(calls[0]).toEqual({
      method: 'ticket.move',
      params: { id: 'e1', lane: 'ui', index: 1 },
    })
    expect(calls[1]).toEqual({
      method: 'ticket.set',
      params: { id: 'e1', title: null, status: 'done', assignee: null, description: null },
    })
    expect(appendIndex(epicBoard, 'ui')).toBe(1)
    expect(dropIndex(epicBoard, 'e1', 'e2', true)).toBe(1)
    const face = source('faces/TicketsFace.tsx')
    expect(face).toContain('dropCalls(')
    expect(face).toContain('LANE_CHANGED')
    expect(face).toContain('snapshot.revision')
  })
})

describe('the empty board', () => {
  it('shows a drop target with a hint in every empty cell, highlighted on drag', () => {
    const cell = source('faces/BoardCell.tsx')
    expect(cell).toContain('board__drop-hint')
    expect(cell).toContain('Drop tickets here')
    expect(cell).toContain('data-empty')
    // The drag highlight survives the hint: the cell still marks its target.
    expect(cell).toContain('data-targeted')
    const css = readFileSync(join(renderer, 'app.css'), 'utf8')
    expect(css).toContain('board__drop-hint')
    expect(css).toContain('board__cell[data-empty]')
  })

  it('keeps the empty copy and reaches every state', () => {
    // TAND-12 keeps the words: the helper owns them and the face draws them.
    const helper = source('shell/board.ts')
    for (const words of ['All clear', 'No matches', 'No tickets match these filters']) {
      expect(helper, `keeps "${words}"`).toContain(words)
    }
    // The face draws its copy through the one helper, so the helper is the
    // seam the reachability below proves.
    const face = source('faces/TicketsFace.tsx')
    expect(face).toContain('emptyBoardCopy(')

    const base = {
      error: null as string | null,
      filters: filters({}),
      nothingVisible: true,
      activeProject: null as string | null,
      laneNames: ['Port'] as readonly string[],
    }
    expect(
      emptyBoardCopy({ ...base, status: 'connecting', ticketCount: 0 }),
    ).toEqual({
      title: 'Reading the board',
      subtitle: 'The store is answering over rpc.',
    })
    expect(
      emptyBoardCopy({ ...base, status: 'failed', ticketCount: 0, error: 'refused' }),
    ).toMatchObject({ title: 'The board could not be read' })
    expect(emptyBoardCopy({ ...base, status: 'ready', ticketCount: 0 })).toMatchObject({
      title: 'Nothing on the board',
    })
    // A board with a visible ticket draws no empty state.
    expect(
      emptyBoardCopy({ ...base, status: 'ready', ticketCount: 2, nothingVisible: false }),
    ).toBeNull()
    // No tickets match the search.
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ text: 'zzz no such ticket' }),
      }),
    ).toMatchObject({ title: 'No matches' })
    // A status, assignee or lane filter hides every ticket.
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ status: 'done' }),
      }),
    ).toMatchObject({ title: 'No tickets match these filters' })
    // The focused lane is empty while the store is not.
    expect(
      emptyBoardCopy({ ...base, status: 'ready', ticketCount: 2, activeProject: 'Port' }),
    ).toMatchObject({ title: 'No tickets in Port' })
    // Nothing open: the work is done.
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ scope: 'open' }),
      }),
    ).toMatchObject({ title: 'All clear' })
    // Nothing completed yet.
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ scope: 'done' }),
      }),
    ).toMatchObject({ title: 'Nothing done yet' })
    // The all scope with nothing in view still answers, so no state is bare.
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ scope: 'all' }),
      }),
    ).toMatchObject({ title: 'The list is empty' })
  })
})

describe('the project header (UI-9)', () => {
  it('draws the project header with its own quiet treatment, distinct from the lane row', () => {
    const layout = source('faces/BoardLayout.tsx')
    // The project header keeps data-project and uses its own quiet group class.
    expect(layout).toContain('board__project')
    expect(layout).toContain('data-project')
    // The header no longer reuses the lane title with a full-width span.
    expect(layout).not.toContain("gridColumn: '1 / -1'")
    expect(layout).not.toMatch(/<h2 className="board__title" style/)
    // Lane rows keep .board__title, including the column head.
    expect(layout).toContain('board__title--head')
    expect(layout).toMatch(/<h2 className="board__title">/)
  })

  it('defines the quiet project treatment without a full-bleed paper band', () => {
    const css = readFileSync(join(appRoot, 'renderer', 'src', 'app.css'), 'utf8')
    const tokens = readFileSync(join(appRoot, 'renderer', 'src', 'tokens.css'), 'utf8')
    const rule = (selector: string): string => {
      const match = css.match(new RegExp(`${selector}\\s*\\{([\\s\\S]*?)\\}`))
      return match?.[1] ?? ''
    }
    const titleRule = rule('\\.board__title')
    const projectRule = rule('\\.board__project-title')
    const countRule = rule('\\.board__project-count')
    expect(projectRule, 'project header has its own rule').not.toBe('')
    // Smaller type than the lane title (12.5px).
    expect(titleRule).toContain('12.5px')
    const laneSize = Number(titleRule.match(/font-size:\s*([\d.]+)px/)?.[1] ?? '0')
    const projectSize = Number(projectRule.match(/font-size:\s*([\d.]+)px/)?.[1] ?? '0')
    expect(projectSize).toBeGreaterThan(0)
    expect(projectSize).toBeLessThan(laneSize)
    // Transparent with no paper band and no lane-title padding.
    expect(projectRule).toContain('transparent')
    expect(projectRule).not.toContain('var(--paper)')
    expect(projectRule).not.toContain('8px 4px')
    // Dim count treatment.
    expect(countRule).toContain('var(--ink-dim)')
    // Token-only values: no colour literals in the new rules.
    for (const body of [projectRule, countRule]) {
      expect(body).not.toMatch(/#[0-9a-fA-F]{3,8}\b/)
      expect(body).not.toMatch(/\brgba?\(/)
      expect(body).not.toMatch(/\bhsla?\(/)
    }
    // Every token the new rules name comes from tokens.css.
    for (const body of [projectRule, countRule]) {
      for (const match of body.matchAll(/var\((--[\w-]+)\)/g)) {
        expect(tokens, `${match[1]} is a token`).toContain(match[1])
      }
    }
  })

  it('keeps grouping invariants for the same-name pair', () => {
    const same: TicketSummary[] = [
      ticket({ id: 't1', lane: 'Tandem', status: 'ready' }),
      ticket({ id: 'u1', lane: null, status: 'backlog' }),
    ]
    const declared = ['Tandem', 'ui', 'core']
    // The default area stays first and ungrouped, declared lanes in render order.
    expect(laneKeys(declared, same)).toEqual([null, 'Tandem', 'ui', 'core'])
    const rows = laneRows(declared, same)
    expect(rows.map((row) => row.title)).toEqual(['Uncategorized', 'Tandem', 'ui', 'core'])
    expect(rows[0]?.key).toBeNull()
    const countOf = (key: string | null): number => {
      const row = rows.find((candidate) => candidate.key === key)
      return row?.cells.reduce((sum, cell) => sum + cell.tickets.length, 0) ?? 0
    }
    // Each lane count stays per lane while the header total is the sum over its lanes.
    expect(countOf('Tandem')).toBe(1)
    expect(countOf('ui')).toBe(0)
    expect(countOf('core')).toBe(0)
    expect(countOf('Tandem') + countOf('ui') + countOf('core')).toBe(1)
    // Lanes no project names stay ungrouped after the grouped rows.
    const dangling: TicketSummary[] = [
      ...same,
      ticket({ id: 'h1', lane: 'Hand edit', status: 'ready' }),
    ]
    expect(laneKeys(declared, dangling)).toEqual([null, 'Tandem', 'ui', 'core', 'Hand edit'])
    // Empty projects stay undrawn: lanes with no rows match nothing to group.
    const onlyTandem = laneRows(['Tandem'], same)
    expect(onlyTandem.map((row) => row.key)).toEqual([null, 'Tandem'])
    expect(onlyTandem.some((row) => row.key === 'ui')).toBe(false)
  })
})
