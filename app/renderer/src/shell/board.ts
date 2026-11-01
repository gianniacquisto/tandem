import type { Call } from '../../../src/shared/bridge'
import type { TicketSummary } from '../wire/TicketSummary'

/**
 * The Tickets face's geometry: which rows are in view, how the lanes lay them
 * out, and the index a drop hands `ticket.move`.
 *
 * It is pure and it is the face's only arithmetic, so the list and the board
 * cannot disagree about what a lane holds, and the drag is provable without a
 * window. The rules are the core's (`domain/query.rs`, `domain/layout.rs`,
 * `domain/ordering.rs`), restated here because the renderer gets one listing and
 * narrows it locally: the snapshot is the whole store, so the stored order the
 * drop arithmetic needs is always the same array the rows came from.
 */

export type Scope = 'open' | 'done' | 'all'
export type View = 'list' | 'board'

export type Filters = {
  scope: Scope
  /** A stored status, or null for any. */
  status: string | null
  /** A stored name, `''` for the unassigned, or null for any. */
  assignee: string | null
  /** A lane name, `''` for the default area, or null for any. */
  lane: string | null
  /** Free text over the title; the summary carries no body. */
  text: string
}

export type Counts = { open: number; done: number; all: number }

export type LaneColumn = {
  /** `null` is the default (ungrouped) area. */
  key: string | null
  title: string
  tickets: TicketSummary[]
}

/** One cell of the board grid: a lane's tickets that share a status. */
export type StatusCell = {
  status: string
  label: string
  tickets: TicketSummary[]
}

/** A lane row of status cells, the board's other axis. */
export type LaneRow = {
  /** `null` is the default (ungrouped) area. */
  key: string | null
  title: string
  cells: StatusCell[]
}

/** Where a drop lands on the grid: both axes, and the lane index it hands `ticket.move`. */
export type DropCell = {
  lane: string | null
  status: string
  /** The index in the lane's stored order; `ticket.move` reads it, `ticket.set` ignores it. */
  index: number
}

export const INITIAL_FILTERS: Filters = {
  scope: 'open',
  status: null,
  assignee: null,
  lane: null,
  text: '',
}

/** The six statuses in workflow order, with the words every face draws. */
export const STATUSES: ReadonlyArray<{ value: string; label: string }> = [
  { value: 'backlog', label: 'Backlog' },
  { value: 'on-hold', label: 'On hold' },
  { value: 'ready', label: 'Ready' },
  { value: 'in-progress', label: 'In progress' },
  { value: 'in-review', label: 'In Review' },
  { value: 'done', label: 'Done' },
]

export const UNCATEGORIZED = 'Uncategorized'

export function statusLabel(status: string): string {
  return STATUSES.find((entry) => entry.value === status)?.label ?? status
}

/**
 * The board's six columns: `STATUSES` in workflow order, with the parked
 * `on-hold` cell last. Derived from the one declared list, never a copy of it.
 */
export function boardStatuses(): ReadonlyArray<{ value: string; label: string }> {
  const parked = STATUSES.filter((status) => status.value === 'on-hold')
  return [...STATUSES.filter((status) => status.value !== 'on-hold'), ...parked]
}

/** The reserved assignee: the human, spelled the same in every face. */
const HUMAN = 'human'

/** `human` is the reserved word; an agent is `@name`; nothing is nothing. */
export function assigneeLabel(assignee: string | null): string | null {
  if (assignee === null) return null
  return assignee === HUMAN ? assignee : `@${assignee}`
}

export type AssigneeChoice = { value: string | null; label: string; selected: boolean }

/**
 * The assignee options both faces offer, in the Swift order
 * (`AssigneePicker.options`): Unassigned, `human`, the store's roster (trimmed,
 * blanks dropped, duplicates collapsed), then the name the face is showing when
 * it is off-roster. A name the roster carries cannot be hidden by a stale
 * roster read.
 *
 * The roster is the `roster` setting, never the board's rows (PRT-38): a name
 * with no ticket has no row to be derived from, and a fresh home has no rows at
 * all. The sheet passes the ticket's assignee as `current`; the board's filter
 * passes the name it is narrowing to.
 */
export function assigneeChoices(
  roster: readonly string[],
  current: string | null,
): AssigneeChoice[] {
  const seen = new Set<string>()
  const values: Array<string | null> = [null]
  function add(raw: string | null): void {
    if (raw === null) return
    const name = raw.trim()
    if (name === '' || seen.has(name)) return
    seen.add(name)
    values.push(name)
  }
  add(HUMAN)
  for (const name of roster) add(name)
  add(current)
  return values.map((value) => ({
    value,
    label: value === null ? 'Unassigned' : (assigneeLabel(value) ?? value),
    selected: value === current,
  }))
}

/**
 * The block's one-line mark (`TAN-232`): `Blocked, TAN-42` when exactly one open
 * blocker has a ref to name, `Blocked` when it has none or there are several.
 */
export function blockLabel(
  ticket: Pick<TicketSummary, 'blocked' | 'blockedBy'>,
): string | null {
  if (!ticket.blocked) return null
  const open = ticket.blockedBy.filter((block) => block.status !== 'done')
  if (open.length !== 1) return 'Blocked'
  const [only] = open
  return only?.ref === null || only === undefined ? 'Blocked' : `Blocked, ${only.ref}`
}

/**
 * The core's fold table, entry for entry (`rust/core/src/domain/text.rs`):
 * case- and diacritic-insensitive, with the Latin accents a title actually
 * carries. `fold.test.ts` reads the Rust source and fails when this drifts, so
 * ADR 0008's one rule, one place holds across the two languages. A letter
 * outside the table is still folded by case, never by accent.
 */
export const FOLD: Readonly<Record<string, string>> = {
  à: 'a', á: 'a', â: 'a', ã: 'a', ä: 'a', å: 'a', ā: 'a', ă: 'a', ą: 'a',
  æ: 'ae',
  ç: 'c', ć: 'c', č: 'c',
  ď: 'd',
  è: 'e', é: 'e', ê: 'e', ë: 'e', ē: 'e', ĕ: 'e', ė: 'e', ę: 'e', ě: 'e',
  ĝ: 'g', ğ: 'g',
  ì: 'i', í: 'i', î: 'i', ï: 'i', ĩ: 'i', ī: 'i', į: 'i',
  ł: 'l',
  ñ: 'n', ń: 'n', ň: 'n',
  ò: 'o', ó: 'o', ô: 'o', õ: 'o', ö: 'o', ø: 'o', ō: 'o', ő: 'o',
  œ: 'oe',
  ř: 'r',
  ś: 's', š: 's', ş: 's',
  ß: 'ss',
  ť: 't',
  ù: 'u', ú: 'u', û: 'u', ü: 'u', ũ: 'u', ū: 'u', ů: 'u', ű: 'u',
  ý: 'y', ÿ: 'y',
  ź: 'z', ż: 'z', ž: 'z',
}

/** `text` lowercased with the Latin accents folded, exactly as the core does. */
export function fold(text: string): string {
  let folded = ''
  for (const character of text) {
    const point = character.codePointAt(0) ?? 0
    // A combining mark on its own carries no letter: the base letter already
    // stands for it.
    if (point >= 0x0300 && point <= 0x036f) continue
    for (const lower of character.toLowerCase()) {
      folded += FOLD[lower] ?? lower
    }
  }
  return folded
}

export function titleMatches(needle: string, title: string): boolean {
  const trimmed = needle.trim()
  if (trimmed === '') return true
  return fold(title).includes(fold(trimmed))
}

/**
 * The three scopes, parted on the archive the way the app's are (`Filter.swift`):
 * open and done hide it, all includes it.
 */
export function scopeCounts(tickets: readonly TicketSummary[]): Counts {
  let open = 0
  let done = 0
  for (const ticket of tickets) {
    if (ticket.archived) continue
    if (ticket.done) done += 1
    else open += 1
  }
  return { open, done, all: tickets.length }
}

export function filterTickets(
  tickets: readonly TicketSummary[],
  filters: Filters,
): TicketSummary[] {
  return tickets.filter((ticket) => {
    if (filters.scope === 'open' && (ticket.archived || ticket.done)) return false
    if (filters.scope === 'done' && (ticket.archived || !ticket.done)) return false
    if (filters.status !== null && ticket.status !== filters.status) return false
    if (filters.assignee !== null) {
      const wanted = filters.assignee === '' ? null : filters.assignee
      if (ticket.assignee !== wanted) return false
    }
    if (filters.lane !== null) {
      const wanted = filters.lane === '' ? null : filters.lane
      if (ticket.lane !== wanted) return false
    }
    if (!titleMatches(filters.text, ticket.title)) return false
    return true
  })
}

/**
 * Every lane key in render order: the default area, the declared lanes, then any
 * name only a ticket carries, so a hand edit cannot hide a ticket.
 */
export function laneKeys(
  declared: readonly string[],
  tickets: readonly TicketSummary[],
): Array<string | null> {
  const keys: Array<string | null> = [null]
  for (const name of declared) {
    if (!keys.includes(name)) keys.push(name)
  }
  for (const ticket of tickets) {
    if (ticket.lane !== null && !keys.includes(ticket.lane)) keys.push(ticket.lane)
  }
  return keys
}

/**
 * The lane keys the faces draw under a project focus: the active lane alone.
 * No focus (the show-all escape), a blank one, or a focus naming no lane key
 * all draw every key in render order, so an unset, cleared or stale focus
 * never hides tickets.
 */
export function visibleLaneKeys(
  keys: ReadonlyArray<string | null>,
  active: string | null,
): Array<string | null> {
  if (active === null || active === '') return [...keys]
  if (!keys.includes(active)) return [...keys]
  return [active]
}

/** The board's columns: one per lane key, in render order, empty ones included. */
export function laneColumns(
  declared: readonly string[],
  tickets: readonly TicketSummary[],
): LaneColumn[] {
  return laneKeys(declared, tickets).map((key) => ({
    key,
    title: key ?? UNCATEGORIZED,
    tickets: tickets.filter((ticket) => ticket.lane === key),
  }))
}

/**
 * The board's rows: one per lane key, each holding a cell per board status in
 * `boardStatuses` order. Empty cells and empty lanes stay, so every cell is a
 * drop target. Tickets keep the store's order inside their cell.
 */
export function laneRows(
  declared: readonly string[],
  tickets: readonly TicketSummary[],
): LaneRow[] {
  return laneColumns(declared, tickets).map((column) => ({
    key: column.key,
    title: column.title,
    cells: boardStatuses().map((status) => ({
      status: status.value,
      label: status.label,
      tickets: column.tickets.filter((ticket) => ticket.status === status.value),
    })),
  }))
}

export function laneMembers(
  tickets: readonly TicketSummary[],
  lane: string | null,
): TicketSummary[] {
  return tickets.filter((ticket) => ticket.lane === lane)
}

/** The index a ticket occupies inside its own lane, or null when it is absent. */
export function laneIndexOf(tickets: readonly TicketSummary[], id: string): number | null {
  const found = tickets.find((ticket) => ticket.id === id)
  if (found === undefined) return null
  return laneMembers(tickets, found.lane).findIndex((ticket) => ticket.id === id)
}

/** Where a drop on an empty part of a lane lands: the end of that lane. */
export function appendIndex(tickets: readonly TicketSummary[], lane: string | null): number {
  return laneMembers(tickets, lane).length
}

/**
 * The writes a drop on the grid makes, in the order the store must take them.
 *
 * Each axis has its own verb (`ticket.move` for the lane, `ticket.set` for the
 * status), and a lane change goes first because it is the one that can be
 * anchored where the card was dropped; the status write then follows the card
 * into its new lane. A drop that changes nothing but the order is the same
 * `ticket.move` the board always made. Empty when the dragged id is not in the
 * listing (a stale drag must not write at all).
 */
export function dropCalls(
  tickets: readonly TicketSummary[],
  moving: string,
  cell: DropCell,
): Call[] {
  const ticket = tickets.find((candidate) => candidate.id === moving)
  if (ticket === undefined) return []

  const calls: Call[] = []
  if (ticket.lane !== cell.lane) {
    calls.push({
      method: 'ticket.move',
      params: { id: moving, lane: cell.lane, index: cell.index },
    })
  }
  if (ticket.status !== cell.status) {
    calls.push({
      method: 'ticket.set',
      params: {
        id: moving,
        title: null,
        status: cell.status,
        assignee: null,
        description: null,
      },
    })
  }
  if (calls.length === 0) {
    calls.push({
      method: 'ticket.move',
      params: { id: moving, lane: cell.lane, index: cell.index },
    })
  }
  return calls
}

/**
 * The index to hand `ticket.move` so `moving` lands immediately before
 * (`after === false`) or after the anchor, within the anchor's lane (`ordering.rs`).
 *
 * The anchor is resolved by id, never by a position in a filtered list, so a
 * narrowing cannot land the drop somewhere else; and one is subtracted when the
 * moving ticket already sat before the drop point, because move removes before
 * it inserts.
 */
export function dropIndex(
  tickets: readonly TicketSummary[],
  moving: string,
  anchor: string,
  after: boolean,
): number | null {
  if (moving === anchor) return null
  const anchorTicket = tickets.find((ticket) => ticket.id === anchor)
  if (anchorTicket === undefined) return null

  const members = laneMembers(tickets, anchorTicket.lane)
  const anchorIndex = members.findIndex((ticket) => ticket.id === anchor)
  if (anchorIndex < 0) return null

  let index = anchorIndex + (after ? 1 : 0)
  const movingIndex = members.findIndex((ticket) => ticket.id === moving)
  if (movingIndex >= 0 && movingIndex < index) index -= 1
  return index
}

/**
 * The index a keyboard move of one *visible* row hands `ticket.move`.
 *
 * The arrows move what the human sees, so the neighbour is the next visible row
 * in the same lane, and the index is resolved against the whole lane through
 * `dropIndex` (hidden rows are stepped over, never counted as a move). `null`
 * when the moving row is not in view or the move would leave the lane.
 */
export function visibleMoveIndex(
  tickets: readonly TicketSummary[],
  visible: readonly TicketSummary[],
  moving: string,
  delta: -1 | 1,
): number | null {
  const at = visible.findIndex((ticket) => ticket.id === moving)
  if (at < 0) return null
  const neighbour = visible[at + delta]
  if (neighbour === undefined) return null
  return dropIndex(tickets, moving, neighbour.id, delta > 0)
}

/**
 * The lane `delta` steps from `lane` in the column order. `undefined` is past an
 * end; `null` is the default area, which is a real key.
 */
export function neighbourLane(
  keys: readonly (string | null)[],
  lane: string | null,
  delta: -1 | 1,
): string | null | undefined {
  const at = keys.indexOf(lane)
  if (at < 0) return undefined
  const next = at + delta
  if (next < 0 || next >= keys.length) return undefined
  return keys[next]
}

/**
 * The ticket a plain arrow selects when the face has none: the first card of
 * the first lane (in `delta` order) that holds one, read across that lane's
 * status cells.
 */
export function firstSelection(rows: readonly LaneRow[], delta: -1 | 1): string | null {
  const start = delta > 0 ? 0 : rows.length - 1
  for (let index = start; index >= 0 && index < rows.length; index += delta) {
    const row = rows[index]
    if (row === undefined) continue
    const first = neighbourSelection(row.cells, null, delta)
    if (first !== null) return first
  }
  return null
}

/**
 * The ticket a left or right arrow selects: the next column that holds one, at
 * the same row where it can, clamped to that column's last row.
 */
export function neighbourSelection(
  columns: ReadonlyArray<{ tickets: TicketSummary[] }>,
  selected: string | null,
  delta: -1 | 1,
): string | null {
  if (selected === null) {
    const start = delta > 0 ? 0 : columns.length - 1
    for (let index = start; index >= 0 && index < columns.length; index += delta) {
      const column = columns[index]
      if (column === undefined) continue
      const first = delta > 0 ? column.tickets[0] : column.tickets.at(-1)
      if (first !== undefined) return first.id
    }
    return null
  }

  let column = -1
  let row = 0
  for (let index = 0; index < columns.length; index += 1) {
    const found = columns[index]?.tickets.findIndex((ticket) => ticket.id === selected) ?? -1
    if (found >= 0) {
      column = index
      row = found
      break
    }
  }
  if (column < 0) return null

  for (let index = column + delta; index >= 0 && index < columns.length; index += delta) {
    const tickets = columns[index]?.tickets ?? []
    if (tickets.length === 0) continue
    return tickets[Math.min(row, tickets.length - 1)]?.id ?? selected
  }
  return selected
}
