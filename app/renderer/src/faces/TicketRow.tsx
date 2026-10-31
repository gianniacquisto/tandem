import { useState, type DragEvent } from 'react'

import { assigneeLabel, blockLabel, statusLabel, type View } from '../shell/board'
import type { TicketSummary } from '../wire/TicketSummary'

export type TicketRowProps = {
  ticket: TicketSummary
  selected: boolean
  layout: View
  onSelect: () => void
  onToggle: () => void
  onOpen?: () => void
  /** A drop on this row's top or bottom half. */
  onDropOn: (draggedId: string, anchorId: string, after: boolean) => void
}

/**
 * One ticket, drawn once for both layouts: the checkbox, the title with its
 * blocked and archived marks, the status capsule, the ref, the assignee and the
 * lane. The list lays it flat; the board sets it on a card. The marks are the
 * Swift row's (`LigneRow`, `BoardTicket`), so a person cannot tell the port from
 * the original at a glance.
 */
export function TicketRow({
  ticket,
  selected,
  layout,
  onSelect,
  onToggle,
  onOpen,
  onDropOn,
}: TicketRowProps) {
  const [targeted, setTargeted] = useState(false)
  const assignee = assigneeLabel(ticket.assignee)
  const block = blockLabel(ticket)

  function dragStart(event: DragEvent<HTMLLIElement>): void {
    event.dataTransfer.setData('text/plain', ticket.id)
    event.dataTransfer.effectAllowed = 'move'
  }

  function drop(event: DragEvent<HTMLLIElement>): void {
    event.preventDefault()
    event.stopPropagation()
    setTargeted(false)
    const dragged = event.dataTransfer.getData('text/plain')
    if (dragged === '' || dragged === ticket.id) return
    const bounds = event.currentTarget.getBoundingClientRect()
    const after = event.clientY > bounds.top + bounds.height / 2
    onDropOn(dragged, ticket.id, after)
  }

  return (
    <li
      className={`ticket-row ticket-row--${layout}`}
      data-ticket={ticket.id}
      data-targeted={targeted || undefined}
      data-done={ticket.done || undefined}
      aria-current={selected ? 'true' : undefined}
      draggable
      onDragStart={dragStart}
      onDragOver={(event) => {
        event.preventDefault()
        event.dataTransfer.dropEffect = 'move'
        setTargeted(true)
      }}
      onDragLeave={() => setTargeted(false)}
      onDrop={drop}
      onClick={onSelect}
      onDoubleClick={() => onOpen?.()}
    >
      <button
        type="button"
        className="ticket-row__check"
        role="checkbox"
        aria-checked={ticket.done}
        aria-label={ticket.done ? `Reopen ${ticket.title}` : `Complete ${ticket.title}`}
        onClick={(event) => {
          event.stopPropagation()
          onToggle()
        }}
      />

      <div className="ticket-row__body">
        <div className="ticket-row__line">
          <span className="ticket-row__title">{ticket.title}</span>

          {ticket.archived ? <span className="ticket-row__tag">archived</span> : null}
          {block === null ? null : (
            <span className="ticket-row__tag ticket-row__tag--blocked">{block}</span>
          )}
          {ticket.status === 'backlog' ? null : (
            <span className="ticket-row__status">{statusLabel(ticket.status)}</span>
          )}

          <span className="ticket-row__ref">{ticket.ref}</span>
          {assignee === null ? null : (
            <span
              className={
                ticket.assignee === 'human'
                  ? 'ticket-row__assignee ticket-row__assignee--human'
                  : 'ticket-row__assignee'
              }
            >
              {assignee}
            </span>
          )}
          <span className="ticket-row__lane">{ticket.lane ?? 'No lane'}</span>
        </div>
      </div>
    </li>
  )
}
