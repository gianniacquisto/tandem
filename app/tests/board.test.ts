import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { BoardLayout } from '../renderer/src/faces/BoardLayout'
import {
  INITIAL_FILTERS,
  LANE_TONES,
  appendIndex,
  assigneeChoices,
  assigneeLabel,
  blockLabel,
  boardColumns,
  boardStatuses,
  boardTickets,
  dropCalls,
  dropIndex,
  emptyBoardCopy,
  filterTickets,
  firstSelection,
  focusLanes,
  laneColumns,
  laneKeys,
  laneMembers,
  laneTone,
  neighbourLane,
  neighbourSelection,
  scopeCounts,
  statusLabel,
  stepSelection,
  titleMatches,
  visibleMoveIndex,
  type BoardColumn,
  type BoardNarrowing,
  type Filters,
  type View,
} from '../renderer/src/shell/board'
import type { TicketSummary } from '../renderer/src/wire/TicketSummary'
import type { WorkflowStage } from '../renderer/src/wire/WorkflowStage'

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
  it('starts on the open board, every axis absent and no search', () => {
    expect(INITIAL_FILTERS).toEqual({
      scope: 'open',
      status: null,
      assignee: null,
      lane: null,
      text: '',
    })
  })

  it('counts the scopes the way the list draws them', () => {
    expect(scopeCounts(board)).toEqual({ open: 2, done: 1, all: 4 })
  })

  it('parts open, done and all on the archive the way the list does', () => {
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

  it('groups the list by lane in render order: the default area, declared lanes, then dangling names', () => {
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
  it('the_default_board_draws_selected_for_development_between_backlog_and_ready', () => {
    expect(boardStatuses().map((status) => status.value)).toEqual([
      'backlog',
      'on-hold',
      'selected',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])
    expect(boardColumns(board).map((column) => column.status)).toEqual([
      'backlog',
      'on-hold',
      'selected',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])
    expect(boardColumns(board).map((column) => column.label)).toEqual([
      'Backlog',
      'On hold',
      'Selected for development',
      'Ready',
      'In progress',
      'In Review',
      'Done',
    ])
    const statuses = boardColumns(board).map((column) => column.status)
    expect(statuses.indexOf('selected')).toBe(statuses.indexOf('backlog') + 2)
    expect(statuses.indexOf('selected')).toBe(statuses.indexOf('ready') - 1)
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

  it('draws a custom stage from the active workflow, without a code change', () => {
    const stages: WorkflowStage[] = [
      { key: 'backlog', label: 'Backlog', role: 'intake', binding: null },
      { key: 'selected', label: 'Selected for development', role: 'intake', binding: null },
      { key: 'ready', label: 'Ready', role: 'ready', binding: null },
      { key: 'in-progress', label: 'In progress', role: 'active', binding: null },
      { key: 'in-review', label: 'In Review', role: 'review', binding: null },
      { key: 'done', label: 'Done', role: 'finished', binding: null },
    ]
    const withSelected = [...board, ticket({ id: 'e', title: 'Echo', status: 'selected' })]

    const columns = boardColumns(withSelected, stages)
    expect(columns.map((column) => column.status)).toEqual([
      'backlog',
      'selected',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])
    expect(columns.find((column) => column.status === 'selected')?.label).toBe(
      'Selected for development',
    )
    expect(columns.find((column) => column.status === 'selected')?.tickets.map((t) => t.id)).toEqual([
      'e',
    ])
    expect(statusLabel('selected', stages)).toBe('Selected for development')
  })

  it('keeps an empty stage as a column, so every stage is a drop target', () => {
    const columns = boardColumns(board)
    expect(columns).toHaveLength(7)
    expect(columns.find((column) => column.status === 'on-hold')?.tickets).toEqual([])
  })

  it('an_empty_selected_cell_is_a_drop_target', () => {
    const columns = boardColumns(board)
    expect(columns.find((column) => column.status === 'selected')?.tickets).toEqual([])
    expect(dropCalls(board, 'b', 'selected')).toEqual([
      {
        method: 'ticket.set',
        params: { id: 'b', title: null, status: 'selected', assignee: null, description: null },
      },
    ])
  })

  it('draws every non-archived ticket, and a done ticket sits in Done whatever the scope says', () => {
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

  it('draws one column per stage, with data-status in stage order', () => {
    expect(drawn.match(/class="board__column"/g)).toHaveLength(7)
    const statuses = [...drawn.matchAll(/class="board__column" data-status="([^"]+)"/g)].map(
      (match) => match[1],
    )
    expect(statuses).toEqual([
      'backlog',
      'on-hold',
      'selected',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])
    // The document order is the workflow's.
    const at = (status: string): number => drawn.indexOf(`data-status="${status}"`)
    expect(at('backlog')).toBeLessThan(at('selected'))
    expect(at('selected')).toBeLessThan(at('ready'))
    expect(at('ready')).toBeLessThan(at('done'))
  })

  it('gives each head the stage label and the number of cards in it', () => {
    const heads = [...drawn.matchAll(/<h2 class="board__column-title">([\s\S]*?)<\/h2>/g)].map(
      (match) => match[1] ?? '',
    )
    expect(heads).toHaveLength(7)
    expect(heads[0]).toContain('Backlog')
    expect(heads[2]).toContain('Selected for development')
    expect(heads[3]).toContain('Ready')
    expect(heads[6]).toContain('Done')
    expect(heads[0]).toContain('<span class="board__column-count">1</span>')
    expect(heads[1]).toContain('<span class="board__column-count">0</span>')
    expect(heads[2]).toContain('<span class="board__column-count">0</span>')
    expect(heads[6]).toContain('<span class="board__column-count">1</span>')
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
    expect(drawn.match(/class="board__add-button"/g)).toHaveLength(7)
    expect(drawn.match(/\+ Add a card/g)).toHaveLength(7)
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

  it('steps across columns at the same row, and within a column up and down', () => {
    expect(neighbourSelection(columns, null, 1)).toBe('b')
    expect(stepSelection(columns, 'b', 'column', 1)).toBe('a')
    expect(stepSelection(columns, 'a', 'column', 1)).toBe('c')
    expect(stepSelection(columns, 'a', 'column', -1)).toBe('b')
    // A column of one card has no row to step to.
    expect(stepSelection(columns, 'b', 'row', 1)).toBeNull()
    expect(stepSelection(columns, 'a', 'row', -1)).toBeNull()
  })

  it('a right move from Backlog lands in Selected when it holds a card', () => {
    const withSelected = boardColumns([
      ...board,
      ticket({ id: 'e', title: 'Echo', status: 'selected' }),
    ])
    expect(stepSelection(withSelected, 'b', 'column', 1)).toBe('e')
    expect(stepSelection(withSelected, 'e', 'column', -1)).toBe('b')
  })
})

describe('the list drag arithmetic', () => {
  it('lands before or after the anchor it was dropped on', () => {
    expect(dropIndex(board, 'a', 'c', true)).toBe(1)
    expect(dropIndex(board, 'a', 'c', false)).toBe(0)
  })

  it('subtracts one when the moving ticket already sat before the drop point', () => {
    expect(dropIndex(board, 'c', 'a', true)).toBe(1)
  })

  it('answers the anchor lane, even when the moving ticket came from another lane', () => {
    expect(dropIndex(board, 'b', 'c', true)).toBe(2)
  })

  it('ignores a drop on itself, and answers nothing for an unknown anchor', () => {
    expect(dropIndex(board, 'a', 'a', true)).toBeNull()
    expect(dropIndex(board, 'a', 'missing', true)).toBeNull()
    expect(dropIndex(board, 'missing', 'a', true)).toBe(1)
  })

  it('appends at the end of a lane, and finds a lane\'s members', () => {
    expect(appendIndex(board, 'Port')).toBe(2)
    expect(appendIndex(board, 'Rien')).toBe(0)
    expect(laneMembers(board, 'Port').map((t) => t.id)).toEqual(['a', 'c'])
  })

  it('moves one visible row at a time, stepping over the rows a filter hides', () => {
    const full = [
      ticket({ id: 'a', lane: 'Port' }),
      ticket({ id: 'b', lane: 'Port' }),
      ticket({ id: 'c', lane: 'Port' }),
    ]
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

describe('the empty board', () => {
  it('keeps the empty copy and reaches every state', () => {
    const helper = source('shell/board.ts')
    for (const words of ['All clear', 'No matches', 'No tickets match these filters']) {
      expect(helper, `keeps "${words}"`).toContain(words)
    }
    const face = source('faces/TicketsFace.tsx')
    expect(face).toContain('emptyBoardCopy(')

    const base = {
      error: null as string | null,
      filters: filters({}),
      view: 'list' as View,
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
        filters: filters({ status: 'done' }),
      }),
    ).toMatchObject({ title: 'No tickets match these filters' })
    expect(
      emptyBoardCopy({ ...base, status: 'ready', ticketCount: 2, activeProject: 'Port' }),
    ).toMatchObject({ title: 'No tickets in Port' })
    // The board's add is per column, so its copy names that route, not "above".
    expect(
      emptyBoardCopy({ ...base, view: 'board', status: 'ready', ticketCount: 0 }),
    ).toMatchObject({
      title: 'Nothing on the board',
      subtitle: "Add a card from a column's foot, or from the CLI.",
    })
    expect(
      emptyBoardCopy({
        ...base,
        view: 'board',
        status: 'ready',
        ticketCount: 2,
        activeProject: 'Port',
      }),
    ).toMatchObject({
      title: 'No tickets in Port',
      subtitle: "Add a card from a column's foot, or show every project.",
    })
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ scope: 'open' }),
      }),
    ).toMatchObject({ title: 'All clear' })
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ scope: 'done' }),
      }),
    ).toMatchObject({ title: 'Nothing done yet' })
    expect(
      emptyBoardCopy({
        ...base,
        status: 'ready',
        ticketCount: 2,
        filters: filters({ scope: 'all' }),
      }),
    ).toMatchObject({ title: 'The list is empty' })
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
    expect(drawn.match(/class="board__add-button"/g)).toHaveLength(7)
    expect(drawn).not.toContain('Drop tickets here')
    expect(drawn).not.toContain('board__cell')
  })
})

describe('the board face (UI-33)', () => {
  it('draws the board from the columns and the one project focus', () => {
    const face = source('faces/TicketsFace.tsx')
    expect(face).toContain('boardColumns(')
    expect(face).toContain('boardTickets(')
    expect(face).toContain('focusLanes(')
    expect(face).toContain('dropCalls(')
    expect(face).toContain('firstAdd')
    // The lane-axis drop plan is gone from the face.
    expect(face).not.toContain('LANE_CHANGED')
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
