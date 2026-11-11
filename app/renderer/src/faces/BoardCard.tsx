import type { DragEvent } from 'react'

import { assigneeLabel, blockLabel, laneTone } from '../shell/board'
import type { TicketSummary } from '../wire/TicketSummary'

export type BoardCardProps = {
  ticket: TicketSummary
  selected: boolean
  onSelect: (id: string) => void
  onOpen: (ticket: TicketSummary) => void
  /** Set the lane filter from the chip, without opening the sheet. */
  onLane: (lane: string) => void
}

/**
 * One board card: the lane chip, the title, and a footer of ref, `@assignee`
 * and the blocked mark. The column is the status, so the card carries no
 * completion checkbox, no status pill and no strikethrough; one click opens the
 * sheet, and a drag starts on the card and carries the ticket id.
 *
 * The chip is the lane's only drawing on the board, and its tone is a hash of
 * the lane name over the palette tokens, so the same lane keeps its colour
 * across sessions and no colour is stored.
 */
export function BoardCard({ ticket, selected, onSelect, onOpen, onLane }: BoardCardProps) {
  const assignee = assigneeLabel(ticket.assignee)
  const block = blockLabel(ticket)

  function dragStart(event: DragEvent<HTMLLIElement>): void {
    event.dataTransfer.setData('text/plain', ticket.id)
    event.dataTransfer.effectAllowed = 'move'
  }

  return (
    <li
      className="ticket-card"
      data-ticket={ticket.id}
      aria-current={selected ? 'true' : undefined}
      draggable
      onDragStart={dragStart}
      onClick={() => {
        onSelect(ticket.id)
        onOpen(ticket)
      }}
    >
      {ticket.lane === null ? null : (
        <button
          type="button"
          className="ticket-card__lane"
          data-tone={laneTone(ticket.lane)}
          onClick={(event) => {
            event.stopPropagation()
            onLane(ticket.lane as string)
          }}
        >
          <span className="ticket-card__lane-dot" aria-hidden="true" />
          {ticket.lane}
        </button>
      )}
      <span className="ticket-card__title">{ticket.title}</span>
      <span className="ticket-card__footer">
        {ticket.ref === null ? null : <span className="ticket-card__ref">{ticket.ref}</span>}
        {assignee === null ? null : (
          <span
            className={
              ticket.assignee === 'human'
                ? 'ticket-card__assignee ticket-card__assignee--human'
                : 'ticket-card__assignee'
            }
          >
            {assignee}
          </span>
        )}
        {block === null ? null : <span className="ticket-card__blocked">{block}</span>}
      </span>
    </li>
  )
}
