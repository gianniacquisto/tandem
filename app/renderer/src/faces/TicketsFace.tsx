import { useEffect, useRef, useState, type ReactNode, type RefObject } from 'react'

import type { Method } from '../../../src/shared/bridge'
import type { Write } from '../shell/write'
import {
  INITIAL_FILTERS,
  assigneeChoices,
  boardAssignees,
  boardColumns,
  boardTickets,
  dropCalls,
  emptyBoardCopy,
  firstSelection,
  focusLanes,
  laneKeys,
  neighbourSelection,
  type BoardColumn,
  type Filters,
} from '../shell/board'
import { keyIntent } from '../shell/keys'
import type { SnapshotState } from '../shell/snapshot'
import type { TicketSummary } from '../wire/TicketSummary'
import { BoardLayout } from './BoardLayout'
import { EmptyState } from './EmptyState'
import { Face } from './Face'
import { FocusControl } from './FocusControl'

export type TicketsFaceProps = {
  snapshot: SnapshotState
  /** The stored focus: the project the face draws, or null for every project. */
  activeProject: string | null
  /** Persist a new focus; null clears back to every project. */
  onPickProject: (project: string | null) => void
  onReload: () => void
  /** A write through the shell's one door. */
  onWrite: Write
  onDismiss: () => void
  /** The sheet's subject, whichever face opened it, so one card reads as current. */
  openTicket: string | null
  /** Open the sheet for the ticket: a card or an add receipt with its composer focus. */
  onOpenTicket: (ticket: { id: string }, focus?: 'description' | null) => void
}

/**
 * The Tickets face: the one-axis board (ADR 0043) and its header filter bar.
 *
 * The shell hands it the whole store as a snapshot and re-reads it on every
 * change event; the face narrows that snapshot locally, so the columns and the
 * filter bar read the same array. Every write is a `Call` the CLI also exposes
 * (`ticket.add`, `ticket.set`, `ticket.move`), and a write aimed at a revision
 * the listing no longer shows is refused by the shell's guard rather than
 * landing on top of somebody else's.
 *
 * The board draws one column per fixed status; the header narrows it by project
 * focus, lane, assignee and title search. A lane is a label on the card and a
 * filter, never a row.
 */
export function TicketsFace({
  snapshot,
  activeProject,
  onPickProject,
  onReload,
  onWrite,
  onDismiss,
  openTicket,
  onOpenTicket,
}: TicketsFaceProps) {
  const [filters, setFilters] = useState<Filters>(INITIAL_FILTERS)
  const [selected, setSelected] = useState<string | null>(null)
  // The show-all escape: every project without clearing the stored focus. The
  // helper still owns the stale rule, so a focus that names no project draws
  // every project even while this is off.
  const [showAll, setShowAll] = useState(false)
  // The sheet's subject wins the card's mark: a ticket opened from a note is the
  // one the face is showing, and the keyboard selection is only the face's own.
  const current = openTicket ?? selected
  const search = useRef<HTMLInputElement>(null)
  const firstAdd = useRef<HTMLButtonElement>(null)

  const allKeys = laneKeys(snapshot.lanes, snapshot.tickets)
  // The focus draws its project's lanes alone; null is every lane (no focus,
  // the show-all escape, or a stale focus), so a stale value never hides work.
  const focus = focusLanes(snapshot.projects, showAll ? null : activeProject)
  // The board's set: every non-archived ticket, narrowed by the focus and the
  // header filters.
  const boardVisible = boardTickets(snapshot.tickets, {
    lanes: focus,
    lane: filters.lane,
    assignee: filters.assignee,
    text: filters.text,
  })
  const boardCols = boardColumns(boardVisible)

  // The one helper the sheet's picker draws: Unassigned, `human`, the names
  // the board's rows carry, then the name this filter is narrowing to when it
  // is off-board. A stale filter value still answers its own option.
  const assignees = assigneeChoices(
    boardAssignees(snapshot.tickets),
    filters.assignee === '' ? null : filters.assignee,
  )

  /**
   * The board's per-column add: one `ticket.add` carrying the column's stage
   * and the lane filter's lane (the default area when the filter is off). The
   * receipt's ticket is selected in its column, and the composer stays open.
   */
  async function addCard(status: string, title: string): Promise<boolean> {
    const written = await onWrite(
      {
        method: 'ticket.add',
        params: {
          title,
          status,
          assignee: null,
          lane: filters.lane === '' ? null : filters.lane,
          origin: null,
          description: null,
        },
      },
      snapshot.revision,
    )
    if (!written.ok) return false
    const id = receiptTicket(written.reply)
    if (id !== null) setSelected(id)
    return true
  }

  /** A drop on a board column: one `ticket.set`, and nothing for the same column. */
  function dropStatus(dragged: string, status: string): void {
    const [call] = dropCalls(snapshot.tickets, dragged, status)
    if (call !== undefined) void onWrite(call, snapshot.revision)
  }

  useEffect(() => {
    function handle(event: KeyboardEvent): void {
      // The sheet owns the keys while it is open: its Escape closes it, and the
      // face behind it must not also clear a search or drop a notice.
      if (openTicket !== null) return
      const element = event.target instanceof HTMLElement ? event.target : null
      const editable =
        element !== null &&
        (element.tagName === 'INPUT' ||
          element.tagName === 'SELECT' ||
          element.tagName === 'TEXTAREA' ||
          element.isContentEditable)

      const intent = keyIntent({
        key: event.key,
        metaKey: event.metaKey,
        ctrlKey: event.ctrlKey,
        altKey: event.altKey,
        shiftKey: event.shiftKey,
        editable,
      })
      if (intent === null) return

      switch (intent.type) {
        case 'focus-add':
          event.preventDefault()
          firstAdd.current?.focus()
          return
        case 'focus-search':
          event.preventDefault()
          search.current?.focus()
          return
        case 'clear-search':
          if (filters.text !== '') {
            event.preventDefault()
            setFilters((current) => ({ ...current, text: '' }))
          } else if (snapshot.notice !== null) {
            event.preventDefault()
            onDismiss()
          }
          return
        case 'dismiss':
          if (snapshot.notice !== null) {
            event.preventDefault()
            onDismiss()
          }
          return
        case 'open': {
          const ticket = snapshot.tickets.find((candidate) => candidate.id === selected)
          if (ticket !== undefined) {
            event.preventDefault()
            onOpenTicket(ticket)
          }
          return
        }
        case 'select': {
          event.preventDefault()
          const next =
            selected === null
              ? firstSelection(boardCols, intent.delta)
              : intent.axis === 'column'
                ? neighbourSelection(boardCols, selected, intent.delta)
                : rowSelection(boardCols, selected, intent.delta)
          if (next !== null) setSelected(next)
          return
        }
        case 'move-across': {
          const ticket = snapshot.tickets.find((candidate) => candidate.id === selected)
          if (ticket === undefined) return
          // The status is the column, so the horizontal arrows step it.
          const at = boardCols.findIndex((column) => column.status === ticket.status)
          const next = boardCols[at + intent.delta]
          if (at < 0 || next === undefined) return
          event.preventDefault()
          dropStatus(ticket.id, next.status)
          return
        }
      }
    }

    window.addEventListener('keydown', handle)
    return () => window.removeEventListener('keydown', handle)
  }, [
    snapshot,
    filters,
    selected,
    boardCols,
    openTicket,
    onDismiss,
    onOpenTicket,
    onWrite,
  ])

  const empty = emptyState(
    snapshot,
    filters,
    boardVisible.length === 0,
    activeProject,
    onReload,
  )
  // A store that has not answered yet draws its message alone. Once it has, the
  // board's columns are the fixed statuses and exist with zero tickets, so
  // the board draws them (with their add feet) and the copy sits above: the
  // copy names an add the human can reach, and the add shortcut has a target.
  const firstRead = snapshot.tickets.length === 0 && snapshot.status !== 'ready'

  const laneFilter = (
    <select
      className="filter"
      aria-label="Lane"
      value={filters.lane === null ? '' : filters.lane === '' ? ' ' : filters.lane}
      onChange={(event) =>
        setFilters((current) => ({
          ...current,
          lane:
            event.target.value === ''
              ? null
              : event.target.value === ' '
                ? ''
                : event.target.value,
        }))
      }
    >
      <option value="">All lanes</option>
      <option value=" ">No lane</option>
      {allKeys
        .filter((key): key is string => key !== null)
        .map((lane) => (
          <option key={lane} value={lane}>
            {lane}
          </option>
        ))}
    </select>
  )

  const assigneeFilter = (
    <select
      className="filter"
      aria-label="Assignee"
      value={filters.assignee === null ? '' : filters.assignee === '' ? ' ' : filters.assignee}
      onChange={(event) =>
        setFilters((current) => ({
          ...current,
          assignee:
            event.target.value === ''
              ? null
              : event.target.value === ' '
                ? ''
                : event.target.value,
        }))
      }
    >
      <option value="">Anyone</option>
      {assignees.map((choice) => (
        <option key={choice.value ?? 'unassigned'} value={choice.value ?? ' '}>
          {choice.label}
        </option>
      ))}
    </select>
  )

  const focusControl = (
    <FocusControl
      projects={snapshot.projects}
      active={activeProject}
      showAll={showAll}
      onPick={onPickProject}
      onToggleShowAll={() => setShowAll((current) => !current)}
    />
  )

  return (
    <Face wide label="Tickets">
      <div className="tickets tickets--board">
        <div className="tickets__bar tickets__bar--board">
          {focusControl}
          {laneFilter}
          {assigneeFilter}
          <SearchField
            value={filters.text}
            onChange={(text) => setFilters((current) => ({ ...current, text }))}
            onClear={() => setFilters((current) => ({ ...current, text: '' }))}
            inputRef={search}
          />
        </div>

        {firstRead ? (
          empty
        ) : (
          <>
            {empty}
            <BoardLayout
              columns={boardCols}
              selected={current}
              onSelect={setSelected}
              onOpen={onOpenTicket}
              onLane={(lane) => setFilters((current) => ({ ...current, lane }))}
              onDropOn={dropStatus}
              onAdd={addCard}
              firstAddRef={firstAdd}
            />
          </>
        )}
      </div>
    </Face>
  )
}

/**
 * The card a plain up or down arrow selects: the neighbour inside the selected
 * card's column, or null when the step leaves the column so the selection stays.
 */
function rowSelection(
  columns: readonly BoardColumn[],
  selected: string,
  delta: -1 | 1,
): string | null {
  const column = columns.find((candidate) =>
    candidate.tickets.some((ticket) => ticket.id === selected),
  )
  if (column === undefined) return null
  const at = column.tickets.findIndex((ticket) => ticket.id === selected)
  return column.tickets[at + delta]?.id ?? null
}

/** The search field: one control in the board's filter bar. */
function SearchField({
  value,
  onChange,
  onClear,
  inputRef,
}: {
  value: string
  onChange: (text: string) => void
  onClear: () => void
  inputRef: RefObject<HTMLInputElement | null>
}) {
  return (
    <div className="search">
      <svg className="search__icon" viewBox="0 0 12 12" aria-hidden="true">
        <circle cx="5" cy="5" r="3.4" />
        <path d="M7.6 7.6 11 11" />
      </svg>
      <input
        ref={inputRef}
        className="search__input"
        type="search"
        value={value}
        placeholder="Search tickets…"
        aria-label="Search tickets"
        onChange={(event) => onChange(event.target.value)}
      />
      {value === '' ? null : (
        <button
          type="button"
          className="search__clear"
          aria-label="Clear the search"
          onClick={onClear}
        >
          ×
        </button>
      )}
    </div>
  )
}

/** The id an add's receipt carries, or null when the reply is not a ticket. */
function receiptTicket(reply: unknown): string | null {
  if (reply === null || reply === undefined || typeof reply !== 'object') return null
  if (!('ticket' in reply)) return null
  const ticket = reply.ticket
  if (ticket === null || typeof ticket !== 'object' || !('id' in ticket)) return null
  return typeof ticket.id === 'string' ? ticket.id : null
}

/** The one message the face draws when there is nothing to draw, or a read that failed. */
function emptyState(
  snapshot: SnapshotState,
  filters: Filters,
  nothingVisible: boolean,
  activeProject: string | null,
  onReload: () => void,
): ReactNode {
  const copy = emptyBoardCopy({
    status: snapshot.status,
    ticketCount: snapshot.tickets.length,
    error: snapshot.error,
    filters,
    nothingVisible,
    activeProject,
    projectNames: snapshot.projects.map((project) => project.name),
  })
  if (copy === null) return null
  // The failed read is the one state with an action: the helper owns the
  // words, the face only adds the retry.
  if (snapshot.status === 'failed' && snapshot.tickets.length === 0) {
    return (
      <EmptyState title={copy.title} subtitle={copy.subtitle}>
        <button type="button" className="empty__action" onClick={onReload}>
          Try again
        </button>
      </EmptyState>
    )
  }
  if (copy.hero === true) {
    return <EmptyState hero title={copy.title} subtitle={copy.subtitle} />
  }
  return <EmptyState title={copy.title} subtitle={copy.subtitle} />
}
