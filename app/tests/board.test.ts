import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { BoardLayout } from '../renderer/src/faces/BoardLayout'
import {
  INITIAL_FILTERS,
  LANE_TONES,
  assigneeChoices,
  assigneeLabel,
  blockLabel,
  boardColumns,
  boardStatuses,
  boardTickets,
  dropCalls,
  emptyBoardCopy,
  firstSelection,
  focusLanes,
  laneKeys,
  laneTone,
  neighbourSelection,
  statusLabel,
  titleMatches,
  type BoardColumn,
  type BoardNarrowing,
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

const projects = [
  { name: 'Port', lanes: ['Port', 'ui'] },
  { name: 'Tandem', lanes: ['Tandem'] },
]

const filters = (overrides: Partial<Filters>): Filters => ({ ...INITIAL_FILTERS, ...overrides })
const narrowing = (overrides: Partial<BoardNarrowing>): BoardNarrowing => ({
  lanes: null,
  lane: null,
  assignee: null,
  text: '',
  ...overrides,
})

describe('the board view over the snapshot', () => {
  it('starts with every header axis absent and no search', () => {
    expect(INITIAL_FILTERS).toEqual({
      assignee: null,
      lane: null,
      text: '',
    })
  })

  it('keeps no list geometry: the layout switch, the scopes and the list ordering are gone', () => {
    const helper = source('shell/board.ts')
    for (const symbol of [
      'type Scope',
      'type View',
      'scopeCounts',
      'filterTickets',
      'laneColumns',
      'appendIndex',
      'dropIndex',
      'visibleMoveIndex',
      'stepSelection',
      'neighbourLane',
    ]) {
      expect(helper, `board.ts still carries ${symbol}`).not.toContain(symbol)
    }
  })

  it('narrows by lane and assignee, with the empty value naming the empty field', () => {
    expect(boardTickets(board, narrowing({ lane: 'Port' })).map((t) => t.id)).toEqual(['a', 'c'])
    expect(boardTickets(board, narrowing({ lane: '' })).map((t) => t.id)).toEqual(['b'])
    expect(boardTickets(board, narrowing({ assignee: '' })).map((t) => t.id)).toEqual([
      'a',
      'b',
      'c',
    ])
  })

  it('searches the title case- and accent-insensitively, and never reorders', () => {
    const accented = [ticket({ id: 'x', title: 'Crème brûlée' }), ticket({ id: 'y', title: 'Creme' })]

    expect(titleMatches('creme', 'Crème brûlée')).toBe(true)
    expect(titleMatches('CREME', 'creme')).toBe(true)
    expect(boardTickets(accented, narrowing({ text: '  crème ' })).map((t) => t.id)).toEqual([
      'x',
      'y',
    ])
    expect(boardTickets(accented, narrowing({ text: 'brulee' })).map((t) => t.id)).toEqual(['x'])
    expect(boardTickets(board, narrowing({ text: 'a' })).map((t) => t.id)).toEqual(['a', 'b', 'c'])
  })

  it('lists the lane keys in render order: the default area, declared lanes, then dangling names', () => {
    expect(laneKeys(['Port'], board)).toEqual([null, 'Port', 'Tandem'])
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

describe('the board columns', () => {
  it('the_default_board_draws_the_six_fixed_statuses_in_order', () => {
    expect(boardStatuses().map((status) => status.value)).toEqual([
      'backlog',
      'on-hold',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])
    expect(boardColumns(board).map((column) => column.status)).toEqual([
      'backlog',
      'on-hold',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])
    expect(boardColumns(board).map((column) => column.label)).toEqual([
      'Backlog',
      'On hold',
      'Ready',
      'In progress',
      'In Review',
      'Done',
    ])
  })

  it('holds exactly the tickets of its stage, and no ticket appears twice', () => {
    const columns = boardColumns(board)
    expect(columns.find((column) => column.status === 'backlog')?.tickets.map((t) => t.id)).toEqual([
      'b',
    ])
    expect(columns.find((column) => column.status === 'ready')?.tickets.map((t) => t.id)).toEqual([
      'a',
    ])
    expect(columns.find((column) => column.status === 'done')?.tickets.map((t) => t.id)).toEqual([
      'c',
      'd',
    ])
    const ids = columns.flatMap((column) => column.tickets.map((t) => t.id))
    expect(ids).toHaveLength(board.length)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('keeps the store order inside a column, lane by lane', () => {
    const ordered: TicketSummary[] = [
      ticket({ id: 'u1', title: 'First ui', lane: 'ui', status: 'ready' }),
      ticket({ id: 'c1', title: 'Core work', lane: 'core', status: 'ready' }),
      ticket({ id: 'u2', title: 'Second ui', lane: 'ui', status: 'ready' }),
      ticket({ id: 't1', title: 'Triage', lane: 'Tandem', status: 'backlog' }),
    ]
    const ready = boardColumns(ordered).find((column) => column.status === 'ready')
    expect(ready?.tickets.map((t) => t.id)).toEqual(['u1', 'c1', 'u2'])
  })

  it('keeps an empty status as a column, so every status is a drop target', () => {
    const columns = boardColumns(board)
    expect(columns).toHaveLength(6)
    expect(columns.find((column) => column.status === 'on-hold')?.tickets).toEqual([])
  })

  it('an_empty_in_progress_cell_is_a_drop_target', () => {
    const columns = boardColumns(board)
    expect(columns.find((column) => column.status === 'in-progress')?.tickets).toEqual([])
    expect(dropCalls(board, 'b', 'in-progress')).toEqual([
      {
        method: 'ticket.set',
        params: { id: 'b', title: null, status: 'in-progress', assignee: null, description: null },
      },
    ])
  })

  it('draws every non-archived ticket, and a done ticket sits in Done', () => {
    const visible = boardTickets(board, narrowing({}))
    expect(visible.map((t) => t.id)).toEqual(['a', 'b', 'c'])
    const columns = boardColumns(visible)
    expect(columns.find((column) => column.status === 'done')?.tickets.map((t) => t.id)).toEqual([
      'c',
    ])
    expect(columns.flatMap((column) => column.tickets.map((t) => t.id))).not.toContain('d')
  })
})

describe('the project focus', () => {
  it('draws the focused project\'s lanes alone', () => {
    expect(focusLanes(projects, 'Port')).toEqual(['Port', 'ui'])
    expect(boardTickets(board, narrowing({ lanes: focusLanes(projects, 'Port') })).map((t) => t.id))
      .toEqual(['a', 'c'])
  })

  it('draws every lane when nothing is focused: the show-all escape', () => {
    expect(focusLanes(projects, null)).toBeNull()
    expect(boardTickets(board, narrowing({ lanes: focusLanes(projects, null) })).map((t) => t.id))
      .toEqual(['a', 'b', 'c'])
  })

  it('falls back to every lane when the focus names no project, so a stale focus hides nothing', () => {
    expect(focusLanes(projects, 'Retro')).toBeNull()
    expect(focusLanes(projects, '')).toBeNull()
  })

  it('treats a stored lane name as no focus: the focus means a project now', () => {
    // `ui` is a lane of Port, not a project: the stored value names no project.
    expect(focusLanes(projects, 'ui')).toBeNull()
    expect(boardTickets(board, narrowing({ lanes: focusLanes(projects, 'ui') })).map((t) => t.id))
      .toEqual(['a', 'b', 'c'])
  })

  it('narrows by the lane filter, where the empty value is the default area', () => {
    expect(boardTickets(board, narrowing({ lane: 'Port' })).map((t) => t.id)).toEqual(['a', 'c'])
    expect(boardTickets(board, narrowing({ lane: '' })).map((t) => t.id)).toEqual(['b'])
    expect(boardTickets(board, narrowing({ lane: 'ui' }))).toEqual([])
  })
})

describe('a drop on a board column', () => {
  const methods = (calls: ReturnType<typeof dropCalls>) => calls.map((call) => call.method)

  it('issues one ticket.set carrying that column\'s stage', () => {
    const calls = dropCalls(board, 'a', 'in-review')

    expect(methods(calls)).toEqual(['ticket.set'])
    expect(calls[0]).toEqual({
      method: 'ticket.set',
      params: { id: 'a', title: null, status: 'in-review', assignee: null, description: null },
    })
  })

  it('writes nothing for a drop in the column the card already stands in', () => {
    expect(dropCalls(board, 'a', 'ready')).toEqual([])
    expect(dropCalls(board, 'c', 'done')).toEqual([])
  })

  it('writes nothing for a ticket the listing does not hold', () => {
    expect(dropCalls(board, 'missing', 'done')).toEqual([])
  })
})

describe('the board card', () => {
  const columns = boardColumns(boardTickets(board, narrowing({})))
  const html = (overrides: { selected?: string | null } = {}): string =>
    renderToStaticMarkup(
      createElement(BoardLayout, {
        columns,
        selected: overrides.selected ?? null,
        onSelect: () => {},
        onOpen: () => {},
        onLane: () => {},
        onDropOn: () => {},
        onAdd: async () => true,
      }),
    )
  const drawn = html({ selected: 'a' })

  it('draws one column per status, with data-status in status order', () => {
    expect(drawn.match(/class="board__column"/g)).toHaveLength(6)
    const statuses = [...drawn.matchAll(/class="board__column" data-status="([^"]+)"/g)].map(
      (match) => match[1],
    )
    expect(statuses).toEqual([
      'backlog',
      'on-hold',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])
    // The document order is the fixed status order.
    const at = (status: string): number => drawn.indexOf(`data-status="${status}"`)
    expect(at('backlog')).toBeLessThan(at('on-hold'))
    expect(at('on-hold')).toBeLessThan(at('ready'))
    expect(at('ready')).toBeLessThan(at('done'))
  })

  it('gives each head the status label and the number of cards in it', () => {
    const heads = [...drawn.matchAll(/<h2 class="board__column-title">([\s\S]*?)<\/h2>/g)].map(
      (match) => match[1] ?? '',
    )
    expect(heads).toHaveLength(6)
    expect(heads[0]).toContain('Backlog')
    expect(heads[1]).toContain('On hold')
    expect(heads[2]).toContain('Ready')
    expect(heads[5]).toContain('Done')
    expect(heads[0]).toContain('<span class="board__column-count">1</span>')
    expect(heads[1]).toContain('<span class="board__column-count">0</span>')
    expect(heads[2]).toContain('<span class="board__column-count">1</span>')
    expect(heads[5]).toContain('<span class="board__column-count">1</span>')
  })

  it('draws one card per ticket, with its data-ticket and lane chip', () => {
    expect(drawn.match(/class="ticket-card"/g)).toHaveLength(3)
    expect(drawn).toContain('data-ticket="a"')
    expect(drawn).toContain('data-ticket="b"')
    expect(drawn).toContain('data-ticket="c"')
    // a and c carry Port; b is the default area and draws no chip.
    expect(drawn.match(/class="ticket-card__lane"/g)).toHaveLength(2)
    expect(drawn).toContain('data-tone="')
  })

  it('carries no completion checkbox, no status pill and no lane text line', () => {
    expect(drawn).not.toContain('ticket-row__check')
    expect(drawn).not.toContain('ticket-row__status')
    expect(drawn).not.toContain('ticket-row')
    expect(drawn).not.toContain('data-done')
  })

  it('carries its add control in every column foot', () => {
    expect(drawn.match(/class="board__add-button"/g)).toHaveLength(6)
    expect(drawn.match(/\+ Add a card/g)).toHaveLength(6)
  })

  it('opens the sheet on one click and filters from the chip without opening it', () => {
    const card = source('faces/BoardCard.tsx')
    expect(card).toContain('onClick')
    expect(card).toContain('onOpen(ticket)')
    // The chip's own click never reaches the card.
    expect(card).toContain('stopPropagation()')
    expect(card).toContain('onLane(ticket.lane')
  })

  it('hashes the chip tone over the palette tokens, deterministically', () => {
    expect(laneTone('Port')).toBe(laneTone('Port'))
    expect(LANE_TONES).toContain(laneTone('Port'))
    const tones = new Set(['Port', 'Tandem', 'ui', 'core', 'Retro', 'Fleet'].map(laneTone))
    expect(tones.size).toBeGreaterThan(1)

    const css = readFileSync(join(renderer, 'app.css'), 'utf8')
    const tokens = readFileSync(join(renderer, 'tokens.css'), 'utf8')
    for (const tone of LANE_TONES) {
      expect(css, `the ${tone} chip has its rule`).toContain(`[data-tone="${tone}"]`)
      expect(tokens, `--${tone} is a token`).toContain(`--${tone}:`)
    }
    expect(css).not.toMatch(/\.ticket-card__lane\[data-tone\][^{]*\{[^}]*#[0-9a-fA-F]{3,8}\b/)
  })
})

describe('the board selection', () => {
  const columns: BoardColumn[] = boardColumns(boardTickets(board, narrowing({})))

  it('selects the first card of the first column that holds one, in either direction', () => {
    expect(firstSelection(columns, 1)).toBe('b')
    expect(firstSelection(columns, -1)).toBe('c')
  })

  it('steps across columns at the same row, and stays when no column holds one', () => {
    expect(neighbourSelection(columns, null, 1)).toBe('b')
    expect(neighbourSelection(columns, 'b', 1)).toBe('a')
    expect(neighbourSelection(columns, 'a', 1)).toBe('c')
    expect(neighbourSelection(columns, 'a', -1)).toBe('b')
    // Backlog is the first column: there is nowhere left to step.
    expect(neighbourSelection(columns, 'b', -1)).toBe('b')
  })

  it('a right move from Backlog lands in On hold when it holds a card', () => {
    const withOnHold = boardColumns([
      ...board,
      ticket({ id: 'e', title: 'Echo', status: 'on-hold' }),
    ])
    expect(neighbourSelection(withOnHold, 'b', 1)).toBe('e')
    expect(neighbourSelection(withOnHold, 'e', -1)).toBe('b')
  })
})

describe('the assignee lists', () => {
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
    const values = assigneeChoices(['human', 'review'], null).map((choice) => choice.value)
    expect(values).toEqual([null, 'human', 'review'])
  })

  it('draws the filter choices from the names the board rows carry', () => {
    expect(source('faces/TicketsFace.tsx'), 'the face uses the one helper').toContain(
      'assigneeChoices(',
    )
    expect(source('faces/TicketsFace.tsx'), 'the face derives its names from rows').toContain(
      'boardAssignees(snapshot.tickets)',
    )
  })
})

describe('the empty board', () => {
  it('keeps the empty copy and reaches every state', () => {
    const helper = source('shell/board.ts')
    for (const words of ['Nothing on the board', 'No matches', 'No tickets match these filters']) {
      expect(helper, `keeps "${words}"`).toContain(words)
    }
    const face = source('faces/TicketsFace.tsx')
    expect(face).toContain('emptyBoardCopy(')

    const base = {
      error: null as string | null,
      filters: filters({}),
      nothingVisible: true,
      activeProject: null as string | null,
      projectNames: ['Port'] as readonly string[],
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
      subtitle: "Add a card from a column's foot, or from the CLI.",
    })
    expect(
      emptyBoardCopy({ ...base, status: 'ready', ticketCount: 2, nothingVisible: false }),
    ).toBeNull()
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ text: 'zzz no such ticket' }),
      }),
    ).toMatchObject({ title: 'No matches' })
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ assignee: 'human' }),
      }),
    ).toMatchObject({ title: 'No tickets match these filters' })
    expect(
      emptyBoardCopy({ ...base, status: 'ready', ticketCount: 2, activeProject: 'Port' }),
    ).toMatchObject({
      title: 'No tickets in Port',
      subtitle: "Add a card from a column's foot, or show every project.",
    })
    // Nothing visible with no filter, no search and no focus: the store holds
    // only archived tickets, so the board is empty.
    expect(
      emptyBoardCopy({ ...base, status: 'ready', ticketCount: 2 }),
    ).toMatchObject({ title: 'The board is empty' })
  })

  it('draws the add control in an empty column, never a dashed cell', () => {
    const empty = boardColumns([ticket({ id: 'only', status: 'backlog' })])
    const drawn = renderToStaticMarkup(
      createElement(BoardLayout, {
        columns: empty,
        selected: null,
        onSelect: () => {},
        onOpen: () => {},
        onLane: () => {},
        onDropOn: () => {},
        onAdd: async () => true,
      }),
    )
    expect(drawn.match(/class="board__add-button"/g)).toHaveLength(6)
    expect(drawn).not.toContain('Drop tickets here')
    expect(drawn).not.toContain('board__cell')
  })
})

describe('the one-axis board face (CUT-8)', () => {
  it('renders the board and its filter bar, with no list layout or switch', () => {
    const face = source('faces/TicketsFace.tsx')
    expect(face).toContain('<BoardLayout')
    expect(face).toContain('tickets__bar--board')
    for (const gone of ['TicketList', 'TicketRow', 'ViewSwitch', 'SCOPES', 'quick-add']) {
      expect(face, `the face still carries ${gone}`).not.toContain(gone)
    }
    expect(face).not.toContain("view === 'board'")
  })

  it('deletes the list face and its row, and nothing in the renderer imports them', () => {
    expect(existsSync(join(renderer, 'faces/TicketList.tsx'))).toBe(false)
    expect(existsSync(join(renderer, 'faces/TicketRow.tsx'))).toBe(false)
    expect(existsSync(join(appRoot, 'tests/tickets-list.test.ts'))).toBe(false)
    for (const name of filesUnder(renderer, ['.ts', '.tsx'])) {
      const text = readFileSync(name, 'utf8')
      expect(text, `${name} imports the list face`).not.toContain("from './TicketList'")
      expect(text, `${name} imports the list row`).not.toContain("from './TicketRow'")
    }
  })

  it('draws the board columns on an empty board, so its add and the shortcut exist', () => {
    const face = source('faces/TicketsFace.tsx')
    // The store's first read draws its message alone; a ready store always
    // draws the columns, whatever they hold, and the copy sits above them.
    expect(face).toContain('const firstRead =')
    expect(face).toContain('firstRead ?')
    expect(face).toContain('<BoardLayout')
    expect(face).toContain('firstAddRef={firstAdd}')
  })

  it('lists the projects, not the lanes, in the one focus control', () => {
    expect(source('faces/FocusControl.tsx')).toContain('projects.map(')
    expect(source('faces/FocusControl.tsx')).toContain('All projects')
    expect(source('App.tsx')).toContain('projects={snapshot.projects}')
  })
})

function filesUnder(directory: string, extensions: string[]): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry)
    if (statSync(path).isDirectory()) return filesUnder(path, extensions)
    return extensions.some((extension) => path.endsWith(extension)) ? [path] : []
  })
}
