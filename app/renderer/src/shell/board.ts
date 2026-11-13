import type { Method } from '../../../src/shared/bridge'
import type { TicketSummary } from '../wire/TicketSummary'

/**
 * The Tickets face's geometry: which tickets are in view, how the board's
 * columns group them, and the write a drop makes.
 *
 * It is pure and it is the face's only arithmetic, so the board's columns and
 * the filter bar cannot disagree about what the store holds. The rules are the
 * core's (`domain/query.rs`, `domain/layout.rs`), restated here because the
 * renderer gets one listing and narrows it locally.
 *
 * The board's axis is the fixed status: `boardColumns` draws one column per
 * status and `dropCalls` answers the one `ticket.set` a drop makes. A lane is a
 * label on the card and a filter, never a row (ADR 0043).
 */

export type Filters = {
  /** A stored name, `''` for the unassigned, or null for any. */
  assignee: string | null
  /** A lane name, `''` for the default area, or null for any. */
  lane: string | null
  /** Free text over the title; the summary carries no body. */
  text: string
}

/** One column of the board: a workflow stage and the tickets standing in it. */
export type BoardColumn = {
  status: string
  label: string
  tickets: TicketSummary[]
}

/** The board's narrowing: a project focus's lanes, and the header's filters. */
export type BoardNarrowing = {
  /** The lanes a project focus draws, or null for every lane (the show-all escape). */
  lanes: readonly string[] | null
  /** A lane name, `''` for the default area, or null for any. */
  lane: string | null
  /** A stored name, `''` for the unassigned, or null for any. */
  assignee: string | null
  /** Free text over the title. */
  text: string
}

export const INITIAL_FILTERS: Filters = {
  assignee: null,
  lane: null,
  text: '',
}

/** One status choice: a stage's key under its editable label. */
export type StatusChoice = { value: string; label: string }

/**
 * The six fixed statuses in workflow order, with the words every face draws.
 * The list is the domain's (`Status::WORKFLOW`), restated here because the
 * renderer holds no core call for it.
 */
export const STATUSES: ReadonlyArray<StatusChoice> = [
  { value: 'backlog', label: 'Backlog' },
  { value: 'on-hold', label: 'On hold' },
  { value: 'ready', label: 'Ready' },
  { value: 'in-progress', label: 'In progress' },
  { value: 'in-review', label: 'In Review' },
  { value: 'done', label: 'Done' },
]

/** The default area's label: the key a ticket with no lane draws under. */
export const UNCATEGORIZED = 'Uncategorized'

/** The phrase one status draws, or its key when a stored row names an unknown one. */
export function statusLabel(status: string): string {
  return STATUSES.find((entry) => entry.value === status)?.label ?? status
}

/**
 * The board's columns: the six fixed statuses in order. The store validates
 * every write against the same six, so a column never needs a workflow read.
 */
export function boardStatuses(): ReadonlyArray<StatusChoice> {
  return STATUSES
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
 * The assignee names the board's rows carry, in first-appearance order: the
 * filter's choices, derived from the tickets present because the store holds
 * no roster any more.
 */
export function boardAssignees(tickets: readonly TicketSummary[]): string[] {
  const seen = new Set<string>()
  const names: string[] = []
  for (const ticket of tickets) {
    if (ticket.assignee === null) continue
    const name = ticket.assignee.trim()
    if (name === '' || seen.has(name)) continue
    seen.add(name)
    names.push(name)
  }
  return names
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

/** One empty message the Tickets face draws, without the widget around it. */
export type EmptyCopy = {
  title: string
  subtitle: string
  /** The first-run states get the room a quiet one does not. */
  hero?: boolean
}

/**
 * The one message the Tickets face draws when there is nothing to draw, or a
 * read that failed. Pure so the board test proves every state answers: the face
 * only wraps the copy in its widget (the failed read adds its retry).
 */
export function emptyBoardCopy(args: {
  status: 'connecting' | 'ready' | 'failed'
  ticketCount: number
  error: string | null
  filters: Filters
  nothingVisible: boolean
  activeProject: string | null
  projectNames: readonly string[]
}): EmptyCopy | null {
  const { status, ticketCount, error, filters, nothingVisible, activeProject, projectNames } = args
  if (status === 'connecting' && ticketCount === 0) {
    return { title: 'Reading the board', subtitle: 'The store is answering over rpc.' }
  }
  if (status === 'failed' && ticketCount === 0) {
    return { title: 'The board could not be read', subtitle: error ?? 'The store refused the read.' }
  }
  if (ticketCount === 0) {
    return {
      title: 'Nothing on the board',
      subtitle: "Add a card from a column's foot, or from the CLI.",
      hero: true,
    }
  }
  if (!nothingVisible) return null
  if (filters.text.trim() !== '') {
    return { title: 'No matches', subtitle: 'No ticket title matches your search.' }
  }
  if (filters.assignee !== null || filters.lane !== null) {
    return {
      title: 'No tickets match these filters',
      subtitle: 'Clear a filter to see more.',
    }
  }
  if (activeProject !== null && projectNames.includes(activeProject)) {
    return {
      title: `No tickets in ${activeProject}`,
      subtitle: "Add a card from a column's foot, or show every project.",
    }
  }
  return { title: 'The board is empty', subtitle: "Add a card from a column's foot." }
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
 * The lanes a project focus draws: the focused project's lanes in render order,
 * or null for every lane. No focus, a blank one, or a name no project carries
 * all draw every lane, so an unset, cleared or stale focus never hides tickets.
 */
export function focusLanes(
  projects: ReadonlyArray<{ name: string; lanes: string[] }>,
  active: string | null,
): string[] | null {
  if (active === null || active === '') return null
  const project = projects.find((candidate) => candidate.name === active)
  if (project === undefined) return null
  return [...project.lanes]
}

/**
 * The board's set: every ticket not archived, narrowed by the project focus,
 * the lane filter, the assignee filter and the search. The board's columns are
 * the statuses, so a done ticket stands in Done.
 */
export function boardTickets(
  tickets: readonly TicketSummary[],
  narrowing: BoardNarrowing,
): TicketSummary[] {
  return tickets.filter((ticket) => {
    if (ticket.archived) return false
    if (narrowing.lanes !== null) {
      if (ticket.lane === null || !narrowing.lanes.includes(ticket.lane)) return false
    }
    if (narrowing.lane !== null) {
      const wanted = narrowing.lane === '' ? null : narrowing.lane
      if (ticket.lane !== wanted) return false
    }
    if (narrowing.assignee !== null) {
      const wanted = narrowing.assignee === '' ? null : narrowing.assignee
      if (ticket.assignee !== wanted) return false
    }
    return titleMatches(narrowing.text, ticket.title)
  })
}

/**
 * The board's columns: one per fixed status, in order, Done included. Each
 * holds exactly the tickets standing at that status, in the order the store
 * gave them (the store's array is lane-grouped, so a column reads lane by lane
 * and then in each lane's own order), and no ticket appears twice. Empty
 * columns stay, so every status is a drop target.
 */
export function boardColumns(tickets: readonly TicketSummary[]): BoardColumn[] {
  return boardStatuses().map((status) => ({
    status: status.value,
    label: status.label,
    tickets: tickets.filter((ticket) => ticket.status === status.value),
  }))
}

/** The palette tokens a lane chip may draw, in one fixed order. */
export const LANE_TONES = ['moss', 'ember', 'accent', 'sun', 'skin', 'hill', 'sky'] as const

/**
 * The tone a lane's chip draws: a hash of the lane name over the palette
 * tokens, so the same lane keeps its colour across sessions and no colour is
 * stored. A chip's colour is a fact about its name, never a second truth.
 */
export function laneTone(name: string): string {
  let hash = 0
  for (const character of name) {
    hash = (hash * 31 + (character.codePointAt(0) ?? 0)) >>> 0
  }
  return LANE_TONES[hash % LANE_TONES.length] ?? 'accent'
}

/**
 * The one write a drop on a column makes: `ticket.set` carrying that column's
 * stage. A drop in the column the card already stands in writes nothing,
 * because the store's order is per lane and a lane-agnostic column has no
 * order a drop could write. Empty when the dragged id is not in the listing (a
 * stale drag must not write at all).
 */
export function dropCalls(
  tickets: readonly TicketSummary[],
  moving: string,
  status: string,
): Method[] {
  const ticket = tickets.find((candidate) => candidate.id === moving)
  if (ticket === undefined || ticket.status === status) return []
  return [
    {
      method: 'ticket.set',
      params: {
        id: moving,
        title: null,
        status,
        assignee: null,
        description: null,
      },
    },
  ]
}

/**
 * The card a plain arrow selects when the face has none: the first card (in
 * `delta` order) of the first column that holds one.
 */
export function firstSelection(
  columns: readonly BoardColumn[],
  delta: -1 | 1,
): string | null {
  return neighbourSelection(columns, null, delta)
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
