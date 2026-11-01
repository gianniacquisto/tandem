import { useEffect, useRef, useState, type ReactNode } from 'react'

import type { Call } from '../../../src/shared/bridge'
import type { Write } from '../shell/write'
import {
  INITIAL_FILTERS,
  STATUSES,
  appendIndex,
  assigneeChoices,
  boardStatuses,
  dropCalls,
  dropIndex,
  filterTickets,
  firstSelection,
  laneColumns,
  laneKeys,
  laneRows,
  neighbourLane,
  neighbourSelection,
  scopeCounts,
  visibleLaneKeys,
  visibleMoveIndex,
  type DropCell,
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
  /** The stored focus: the lane the face draws, or null for every lane. */
  activeProject: string | null
  /** Persist a new focus; null clears back to every lane. */
  onPickProject: (lane: string | null) => void
  onReload: () => void
  /** A write through the shell's one door. */
  onWrite: Write
  onDismiss: () => void
  /** The sheet's subject, whichever face opened it, so one row reads as current. */
  openTicket: string | null
  onOpenTicket: (ticket: TicketSummary) => void
}

const SCOPES: ReadonlyArray<{ value: Scope; label: string }> = [
  { value: 'open', label: 'Open' },
  { value: 'done', label: 'Done' },
  { value: 'all', label: 'All' },
]

/** A status write that follows a landed lane write: the lane half is already stored. */
const LANE_CHANGED = 'The lane changed, but the status did not.'

/**
 * The Tickets face: one listing, two layouts, one row widget.
 *
 * The shell hands it the whole store as a snapshot and re-reads it on every
 * change event; the face narrows that snapshot locally, so the scope counts and
 * the drag arithmetic are the same array the rows came from. Every write is a
 * `Call` the CLI also exposes (`ticket.add`, `ticket.complete`, `ticket.set`,
 * `ticket.move`), and a write aimed at a revision the listing no longer shows is
 * refused by the shell's guard rather than landing on top of somebody else's.
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
  // The show-all escape: every lane without clearing the stored focus. The
  // helper still owns the stale rule, so a focus that names no lane key draws
  // every lane even while this is off.
  const [showAll, setShowAll] = useState(false)
  // The sheet's subject wins the row's mark: a ticket opened from a note is the
  // one the face is showing, and the keyboard selection is only the face's own.
  const current = openTicket ?? selected
  const [draft, setDraft] = useState('')
  const [addLane, setAddLane] = useState<string | null>(null)
  const quickAdd = useRef<HTMLInputElement>(null)
  const search = useRef<HTMLInputElement>(null)

  const allKeys = laneKeys(snapshot.lanes, snapshot.tickets)
  // The focus draws its lane alone; the columns and rows are the same helpers
  // narrowed to the visible keys, so an empty focused lane stays a drop target
  // and a stale focus draws every lane.
  const keys = visibleLaneKeys(allKeys, showAll ? null : activeProject)
  const counts = scopeCounts(snapshot.tickets.filter((ticket) => keys.includes(ticket.lane)))
  const visible = filterTickets(snapshot.tickets, filters)
  const columns = laneColumns(snapshot.lanes, visible).filter((column) =>
    keys.includes(column.key),
  )
  const rows = laneRows(snapshot.lanes, visible).filter((row) => keys.includes(row.key))
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
    if (written.ok) {
      setDraft('')
      quickAdd.current?.focus()
    }
  }

  function toggle(ticket: TicketSummary): void {
    const call: Call = ticket.done
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

  /**
   * The drop's writes, in the planner's order. A second write is a follow-up:
   * the first one moved the listing, so it is exempt from the revision guard
   * and its refusal says that the lane change already landed.
   */
  async function dropAt(dragged: string, cell: DropCell): Promise<void> {
    const calls = dropCalls(snapshot.tickets, dragged, cell)
    for (const [at, call] of calls.entries()) {
      const written = await onWrite(
        call,
        at === 0 ? snapshot.revision : null,
        at === 0 ? undefined : LANE_CHANGED,
      )
      if (!written.ok) return
    }
  }

  function dropOn(dragged: string, anchor: string, after: boolean): void {
    const index = dropIndex(snapshot.tickets, dragged, anchor, after)
    const target = snapshot.tickets.find((ticket) => ticket.id === anchor)
    if (index === null || target === undefined) return
    void dropAt(dragged, { lane: target.lane, status: target.status, index })
  }

  function dropCell(dragged: string, lane: string | null, status: string): void {
    void dropAt(dragged, { lane, status, index: appendIndex(snapshot.tickets, lane) })
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
          quickAdd.current?.focus()
          return
        case 'focus-search':
          event.preventDefault()
          search.current?.focus()
          return
        case 'scope':
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
            if (selected === null) {
              setSelected(firstSelection(rows, intent.delta))
              return
            }
            const rowIndex = rows.findIndex((row) =>
              row.cells.some((cell) => cell.tickets.some((ticket) => ticket.id === selected)),
            )
            const cellIndex =
              rows[rowIndex]?.cells.findIndex((cell) =>
                cell.tickets.some((ticket) => ticket.id === selected),
              ) ?? -1
            if (rowIndex < 0 || cellIndex < 0) {
              setSelected(firstSelection(rows, intent.delta))
              return
            }
            // Left and right step the status cells of this lane; up and down
            // step the lanes at the same status.
            const axis =
              intent.axis === 'column'
                ? (rows[rowIndex]?.cells ?? [])
                : rows.map(
                    (row) => row.cells[cellIndex] ?? { status: '', label: '', tickets: [] },
                  )
            setSelected(neighbourSelection(axis, selected, intent.delta))
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
          const ticket = snapshot.tickets.find((candidate) => candidate.id === selected)
          if (ticket === undefined) return
          if (view === 'board') {
            // The lane is the row, so the vertical arrows step it.
            const lane = neighbourLane(allKeys, ticket.lane, intent.delta)
            if (lane === undefined) return
            event.preventDefault()
            void dropAt(ticket.id, {
              lane,
              status: ticket.status,
              index: appendIndex(snapshot.tickets, lane),
            })
            return
          }
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
            const statuses = boardStatuses()
            const at = statuses.findIndex((status) => status.value === ticket.status)
            const next = statuses[at + intent.delta]
            if (at < 0 || next === undefined) return
            event.preventDefault()
            void dropAt(ticket.id, {
              lane: ticket.lane,
              status: next.value,
              index: appendIndex(snapshot.tickets, ticket.lane),
            })
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
    rows,
    order,
    allKeys,
    openTicket,
    onDismiss,
    onOpenTicket,
  ])

  const empty = emptyState(snapshot, filters, visible.length === 0, activeProject, onReload)

  return (
    <Face wide title="Tickets" blurb="The board, its lanes and the ticket list, one face.">
      <div className={view === 'board' ? 'tickets tickets--board' : 'tickets tickets--list'}>
        <FocusControl
          lanes={snapshot.lanes}
          active={activeProject}
          showAll={showAll}
          onPick={onPickProject}
          onToggleShowAll={() => setShowAll((current) => !current)}
        />
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
            onChange={(event) => setAddLane(event.target.value === '' ? null : event.target.value)}
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
                onClick={() => setFilters((current) => ({ ...current, scope: scope.value }))}
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
              {STATUSES.map((status) => (
                <option key={status.value} value={status.value}>
                  {status.label}
                </option>
              ))}
            </select>

            <select
              className="filter"
              aria-label="Assignee"
              value={
                filters.assignee === null ? '' : filters.assignee === '' ? ' ' : filters.assignee
              }
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
          </div>

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
        </div>

        <div className="search">
          <svg className="search__icon" viewBox="0 0 12 12" aria-hidden="true">
            <circle cx="5" cy="5" r="3.4" />
            <path d="M7.6 7.6 11 11" />
          </svg>
          <input
            ref={search}
            className="search__input"
            type="search"
            value={filters.text}
            placeholder="Search tickets…"
            aria-label="Search tickets"
            onChange={(event) =>
              setFilters((current) => ({ ...current, text: event.target.value }))
            }
          />
          {filters.text === '' ? null : (
            <button
              type="button"
              className="search__clear"
              aria-label="Clear the search"
              onClick={() => setFilters((current) => ({ ...current, text: '' }))}
            >
              ×
            </button>
          )}
        </div>

        {empty ?? (
          view === 'list' ? (
            <TicketList
              columns={columns}
              selected={current}
              onSelect={setSelected}
              onToggle={toggle}
              onOpen={onOpenTicket}
              onDropOn={dropOn}
            />
          ) : (
            <BoardLayout
              rows={rows}
              selected={current}
              onSelect={setSelected}
              onToggle={toggle}
              onOpen={onOpenTicket}
              onDropOn={dropOn}
              onDropCell={dropCell}
            />
          )
        )}
      </div>
    </Face>
  )
}

/** The one message the face draws when there is nothing to draw, or a read that failed. */
function emptyState(
  snapshot: SnapshotState,
  filters: Filters,
  nothingVisible: boolean,
  activeProject: string | null,
  onReload: () => void,
): ReactNode {
  if (snapshot.status === 'connecting' && snapshot.tickets.length === 0) {
    return <EmptyState title="Reading the board" subtitle="The store is answering over rpc." />
  }
  if (snapshot.status === 'failed' && snapshot.tickets.length === 0) {
    return (
      <EmptyState
        title="The board could not be read"
        subtitle={snapshot.error ?? 'The store refused the read.'}
      >
        <button type="button" className="empty__action" onClick={onReload}>
          Try again
        </button>
      </EmptyState>
    )
  }
  if (snapshot.tickets.length === 0) {
    return (
      <EmptyState
        hero
        title="Nothing on the board"
        subtitle="Add a ticket above, or from the CLI."
      />
    )
  }
  if (!nothingVisible) return null
  if (filters.text.trim() !== '') {
    return <EmptyState title="No matches" subtitle="No ticket title matches your search." />
  }
  if (filters.status !== null || filters.assignee !== null || filters.lane !== null) {
    return (
      <EmptyState title="No tickets match these filters" subtitle="Clear a filter to see more." />
    )
  }
  if (activeProject !== null && snapshot.lanes.includes(activeProject)) {
    return (
      <EmptyState
        title={`No tickets in ${activeProject}`}
        subtitle="Add one above, or show every lane."
      />
    )
  }
  if (filters.scope === 'open') {
    return <EmptyState hero title="All clear" subtitle="Nothing open. Add something above." />
  }
  if (filters.scope === 'done') {
    return <EmptyState title="Nothing done yet" subtitle="Completed tickets rest here." />
  }
  return <EmptyState title="The list is empty" subtitle="Add your first ticket above." />
}
