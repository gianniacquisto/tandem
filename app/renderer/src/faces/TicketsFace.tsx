import { useEffect, useRef, useState, type ReactNode, type RefObject } from 'react'

import type { Method } from '../../../src/shared/bridge'
import type { Write } from '../shell/write'
import {
  INITIAL_FILTERS,
  appendIndex,
  assigneeChoices,
  boardColumns,
  boardStatuses,
  boardTickets,
  dropCalls,
  dropIndex,
  emptyBoardCopy,
  filterTickets,
  focusLanes,
  laneKeys,
  laneColumns,
  neighbourLane,
  neighbourSelection,
  scopeCounts,
  stepSelection,
  visibleMoveIndex,
  type Filters,
  type Scope,
  type View,
} from '../shell/board'
import { keyIntent } from '../shell/keys'
import { viewHref } from '../shell/route'
import type { SnapshotState } from '../shell/snapshot'
import type { TicketSummary } from '../wire/TicketSummary'
import { BoardLayout } from './BoardLayout'
import { EmptyState } from './EmptyState'
import { Face } from './Face'
import { FocusControl } from './FocusControl'
import { TicketList } from './TicketList'

export type TicketsFaceProps = {
  snapshot: SnapshotState
  view: View
  /** The store's roster: the names the assignee filter offers (`settings.get`). */
  roster: string[]
  /** The stored focus: the project the face draws, or null for every project. */
  activeProject: string | null
  /** Persist a new focus; null clears back to every project. */
  onPickProject: (project: string | null) => void
  onReload: () => void
  /** A write through the shell's one door. */
  onWrite: Write
  onDismiss: () => void
  /** The sheet's subject, whichever face opened it, so one row reads as current. */
  openTicket: string | null
  /** Open the sheet for the ticket: a card, a row, or a quick-add receipt with its composer focus. */
  onOpenTicket: (ticket: { id: string }, focus?: 'description' | null) => void
}

const SCOPES: ReadonlyArray<{ value: Scope; label: string }> = [
  { value: 'open', label: 'Open' },
  { value: 'done', label: 'Done' },
  { value: 'all', label: 'All' },
]

/**
 * The Tickets face: one listing, two layouts, one row widget.
 *
 * The shell hands it the whole store as a snapshot and re-reads it on every
 * change event; the face narrows that snapshot locally, so the scope counts and
 * the drag arithmetic are the same array the rows came from. Every write is a
 * `Call` the CLI also exposes (`ticket.add`, `ticket.complete`, `ticket.set`,
 * `ticket.move`), and a write aimed at a revision the listing no longer shows is
 * refused by the shell's guard rather than landing on top of somebody else's.
 *
 * The board draws one column per workflow stage and ignores the Open/Done/All
 * scope: the columns are the statuses. The list keeps its scopes, its status
 * filter and its quick-add. Both follow the one stored project focus.
 */
export function TicketsFace({
  snapshot,
  view,
  roster,
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
  const [draft, setDraft] = useState('')
  const [addLane, setAddLane] = useState<string | null>(null)
  const quickAdd = useRef<HTMLInputElement>(null)
  const search = useRef<HTMLInputElement>(null)
  const firstAdd = useRef<HTMLButtonElement>(null)

  const allKeys = laneKeys(snapshot.lanes, snapshot.tickets)
  // The focus draws its project's lanes alone; null is every lane (no focus,
  // the show-all escape, or a stale focus), so a stale value never hides work.
  const focus = focusLanes(snapshot.projects, showAll ? null : activeProject)
  const focusedTickets =
    focus === null
      ? snapshot.tickets
      : snapshot.tickets.filter(
          (ticket) => ticket.lane !== null && focus.includes(ticket.lane),
        )
  const counts = scopeCounts(focusedTickets)
  const listVisible = filterTickets(snapshot.tickets, filters)
  const columns = laneColumns(snapshot.lanes, listVisible).filter((column) =>
    focus === null ? true : column.key !== null && focus.includes(column.key),
  )
  const stages = snapshot.workflow?.stages ?? null
  const statuses = boardStatuses(stages)
  // The board's set: every non-archived ticket, narrowed by the focus and the
  // header filters. The scope and the status filter are the list's.
  const boardVisible = boardTickets(snapshot.tickets, {
    lanes: focus,
    lane: filters.lane,
    assignee: filters.assignee,
    text: filters.text,
  })
  const boardCols = boardColumns(boardVisible, stages)
  const order = columns.flatMap((column) => column.tickets)

  // The one helper the sheet's picker draws: Unassigned, `human`, the store's
  // roster, then the name this filter is narrowing to when it is off-roster
  // (PRT-38). The rows are not the source: a stored name with no ticket has no
  // row to be derived from.
  const assignees = assigneeChoices(roster, filters.assignee === '' ? null : filters.assignee)

  function scopeCount(scope: Scope): number {
    return scope === 'open' ? counts.open : scope === 'done' ? counts.done : counts.all
  }

  async function add(): Promise<void> {
    const title = draft.trim()
    if (title === '') return
    const written = await onWrite(
      {
        method: 'ticket.add',
        params: {
          title,
          status: null,
          assignee: null,
          lane: addLane,
          origin: null,
          description: null,
        },
      },
      snapshot.revision,
    )
    if (!written.ok) return
    setDraft('')
    // A successful add opens its own sheet in the description composer: the
    // receipt already carries the ticket, never a second list read. Esc or the
    // close control returns via the existing dismiss.
    const id = receiptTicket(written.reply)
    if (id !== null) {
      onOpenTicket({ id }, 'description')
      return
    }
    quickAdd.current?.focus()
  }

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

  function toggle(ticket: TicketSummary): void {
    const call: Method = ticket.done
      ? {
          method: 'ticket.set',
          params: {
            id: ticket.id,
            title: null,
            status: 'backlog',
            assignee: null,
            description: null,
          },
        }
      : { method: 'ticket.complete', params: { id: ticket.id } }
    void onWrite(call, snapshot.revision)
  }

  function move(id: string, lane: string | null, index: number): void {
    void onWrite({ method: 'ticket.move', params: { id, lane, index } }, snapshot.revision)
  }

  /** A drop on a card in the list: the lane write and its index in that lane. */
  function dropOn(dragged: string, anchor: string, after: boolean): void {
    const index = dropIndex(snapshot.tickets, dragged, anchor, after)
    const target = snapshot.tickets.find((ticket) => ticket.id === anchor)
    if (index === null || target === undefined) return
    move(dragged, target.lane, index)
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
        case 'focus-quick-add':
          event.preventDefault()
          if (view === 'board') firstAdd.current?.focus()
          else quickAdd.current?.focus()
          return
        case 'focus-search':
          event.preventDefault()
          search.current?.focus()
          return
        case 'scope':
          // The scope switch is the list's; the board's columns are the statuses.
          if (view === 'board') return
          event.preventDefault()
          setFilters((current) => ({ ...current, scope: intent.scope }))
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
          if (view === 'board') {
            const next = stepSelection(boardCols, selected, intent.axis, intent.delta)
            if (next !== null) setSelected(next)
            return
          }
          if (intent.axis === 'column') {
            setSelected(neighbourSelection(columns, selected, intent.delta))
            return
          }
          const at = order.findIndex((ticket) => ticket.id === selected)
          const next =
            at < 0 ? order[intent.delta > 0 ? 0 : order.length - 1] : order[at + intent.delta]
          if (next !== undefined) setSelected(next.id)
          return
        }
        case 'move-in-lane': {
          // The lane is a label now: ordering stays the list's, so the vertical
          // arrows do nothing on the board.
          if (view === 'board') return
          const ticket = snapshot.tickets.find((candidate) => candidate.id === selected)
          if (ticket === undefined) return
          const lane = columns.find((column) => column.key === ticket.lane)?.tickets ?? []
          const target = visibleMoveIndex(snapshot.tickets, lane, ticket.id, intent.delta)
          if (target === null) return
          event.preventDefault()
          move(ticket.id, ticket.lane, target)
          return
        }
        case 'move-across': {
          const ticket = snapshot.tickets.find((candidate) => candidate.id === selected)
          if (ticket === undefined) return
          if (view === 'board') {
            // The status is the column, so the horizontal arrows step it.
            const at = boardCols.findIndex((column) => column.status === ticket.status)
            const next = boardCols[at + intent.delta]
            if (at < 0 || next === undefined) return
            event.preventDefault()
            dropStatus(ticket.id, next.status)
            return
          }
          const lane = neighbourLane(allKeys, ticket.lane, intent.delta)
          if (lane === undefined) return
          event.preventDefault()
          move(ticket.id, lane, appendIndex(snapshot.tickets, lane))
          return
        }
      }
    }

    window.addEventListener('keydown', handle)
    return () => window.removeEventListener('keydown', handle)
  }, [
    snapshot,
    filters,
    view,
    selected,
    columns,
    boardCols,
    order,
    allKeys,
    openTicket,
    onDismiss,
    onOpenTicket,
  ])

  const empty = emptyState(
    snapshot,
    filters,
    view === 'board' ? boardVisible.length === 0 : listVisible.length === 0,
    activeProject,
    onReload,
  )

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
      <div className={view === 'board' ? 'tickets tickets--board' : 'tickets tickets--list'}>
        {view === 'board' ? (
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
            <ViewSwitch view={view} />
          </div>
        ) : (
          <>
            {focusControl}
            <form
              className="quick-add"
              onSubmit={(event) => {
                event.preventDefault()
                void add()
              }}
            >
              <span className="quick-add__plus" aria-hidden="true">
                +
              </span>
              <input
                ref={quickAdd}
                className="quick-add__input"
                value={draft}
                placeholder="Add a ticket…"
                aria-label="Add a ticket"
                onChange={(event) => setDraft(event.target.value)}
              />
              <select
                className="quick-add__lane"
                aria-label="Lane for a new ticket"
                value={addLane ?? ''}
                onChange={(event) =>
                  setAddLane(event.target.value === '' ? null : event.target.value)
                }
              >
                <option value="">No lane</option>
                {snapshot.lanes.map((lane) => (
                  <option key={lane} value={lane}>
                    {lane}
                  </option>
                ))}
              </select>
            </form>

            <div className="tickets__bar">
              <div className="segmented" role="tablist" aria-label="Ticket scopes">
                {SCOPES.map((scope) => (
                  <button
                    key={scope.value}
                    type="button"
                    role="tab"
                    className="segmented__item"
                    aria-selected={filters.scope === scope.value}
                    onClick={() =>
                      setFilters((current) => ({ ...current, scope: scope.value }))
                    }
                  >
                    <span>{scope.label}</span>
                    <span className="segmented__count">{scopeCount(scope.value)}</span>
                  </button>
                ))}
              </div>

              <div className="tickets__filters">
                <select
                  className="filter"
                  aria-label="Status"
                  value={filters.status ?? ''}
                  onChange={(event) =>
                    setFilters((current) => ({
                      ...current,
                      status: event.target.value === '' ? null : event.target.value,
                    }))
                  }
                >
                  <option value="">Any status</option>
                  {statuses.map((status) => (
                    <option key={status.value} value={status.value}>
                      {status.label}
                    </option>
                  ))}
                </select>
                {assigneeFilter}
                {laneFilter}
              </div>

              <SearchField
                value={filters.text}
                onChange={(text) => setFilters((current) => ({ ...current, text }))}
                onClear={() => setFilters((current) => ({ ...current, text: '' }))}
                inputRef={search}
              />
              <ViewSwitch view={view} />
            </div>
          </>
        )}

        {empty ?? (
          view === 'list' ? (
            <TicketList
              columns={columns}
              stages={stages}
              selected={current}
              onSelect={setSelected}
              onToggle={toggle}
              onOpen={onOpenTicket}
              onDropOn={dropOn}
            />
          ) : (
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
          )
        )}
      </div>
    </Face>
  )
}

/** The search field: one control, drawn in the board's row and the list's. */
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

/** The List/Board switch: one control, drawn in both rows. */
function ViewSwitch({ view }: { view: View }) {
  return (
    <div className="segmented segmented--views" role="tablist" aria-label="Ticket layouts">
      <a
        className="segmented__item"
        role="tab"
        href={viewHref('tickets', 'list')}
        aria-selected={view === 'list'}
      >
        List
      </a>
      <a
        className="segmented__item"
        role="tab"
        href={viewHref('tickets', 'board')}
        aria-selected={view === 'board'}
      >
        Board
      </a>
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
