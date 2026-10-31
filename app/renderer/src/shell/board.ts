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

/** `human` is the reserved word; an agent is `@name`; nothing is nothing. */
export function assigneeLabel(assignee: string | null): string | null {
  if (assignee === null) return null
  return assignee === 'human' ? assignee : `@${assignee}`
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

/** Case- and diacritic-insensitive, the comparison the core's search uses. */
export function fold(text: string): string {
  return text
    .toLowerCase()
    .replace(/æ/g, 'ae')
    .replace(/œ/g, 'oe')
    .replace(/ø/g, 'o')
    .replace(/ß/g, 'ss')
    .replace(/ð/g, 'd')
    .replace(/þ/g, 'th')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
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
 * The ticket a left or right arrow selects: the next column that holds one, at
 * the same row where it can, clamped to that column's last row.
 */
export function neighbourSelection(
  columns: readonly LaneColumn[],
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
