import { useState, type DragEvent } from 'react'

import type { LaneColumn } from '../shell/board'
import type { TicketSummary } from '../wire/TicketSummary'
import { TicketRow } from './TicketRow'

export type BoardColumnProps = {
  column: LaneColumn
  selected: string | null
  onSelect: (id: string) => void
  onToggle: (ticket: TicketSummary) => void
  onOpen?: (ticket: TicketSummary) => void
  onDropOn: (draggedId: string, anchorId: string, after: boolean) => void
  onDropInto: (draggedId: string, lane: string | null) => void
}

/**
 * One lane column: its title and count, then its tickets as cards. The whole
 * column is a drop target, so a lane with no cards still draws and still takes
 * the next drop; a drop on a card is the card's own, and it stops here.
 */
export function BoardColumn({
  column,
  selected,
  onSelect,
  onToggle,
  onOpen,
  onDropOn,
  onDropInto,
}: BoardColumnProps) {
  const [targeted, setTargeted] = useState(false)

  function drop(event: DragEvent<HTMLElement>): void {
    event.preventDefault()
    setTargeted(false)
    const dragged = event.dataTransfer.getData('text/plain')
    if (dragged !== '') onDropInto(dragged, column.key)
  }

  return (
    <section
      className="board__column"
      aria-label={column.title}
      data-lane={column.key ?? ''}
      data-targeted={targeted || undefined}
      onDragOver={(event) => {
        event.preventDefault()
        event.dataTransfer.dropEffect = 'move'
        setTargeted(true)
      }}
      onDragLeave={() => setTargeted(false)}
      onDrop={drop}
    >
      <h2 className="board__title">
        <span>{column.title}</span>
        <span className="board__count">{column.tickets.length}</span>
      </h2>
      <ul className="board__tickets">
        {column.tickets.map((ticket) => (
          <TicketRow
            key={ticket.id}
            ticket={ticket}
            layout="board"
            selected={ticket.id === selected}
            onSelect={() => onSelect(ticket.id)}
            onToggle={() => onToggle(ticket)}
            onOpen={onOpen === undefined ? undefined : () => onOpen(ticket)}
            onDropOn={onDropOn}
          />
        ))}
      </ul>
    </section>
  )
}
