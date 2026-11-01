import { useState, type DragEvent } from 'react'

import type { LaneRow, StatusCell } from '../shell/board'
import type { TicketSummary } from '../wire/TicketSummary'
import { TicketRow } from './TicketRow'

export type BoardCellProps = {
  lane: LaneRow
  cell: StatusCell
  selected: string | null
  onSelect: (id: string) => void
  onToggle: (ticket: TicketSummary) => void
  onOpen?: (ticket: TicketSummary) => void
  onDropOn: (draggedId: string, anchorId: string, after: boolean) => void
  onDropCell: (draggedId: string, lane: string | null, status: string) => void
}

/**
 * One cell of the board: a lane's tickets at one status, in the store's order.
 * The whole cell is a drop target, so a status with no cards still takes the
 * next drop; a drop on a card is the card's own, and it stops here.
 */
export function BoardCell({
  lane,
  cell,
  selected,
  onSelect,
  onToggle,
  onOpen,
  onDropOn,
  onDropCell,
}: BoardCellProps) {
  const [targeted, setTargeted] = useState(false)

  function drop(event: DragEvent<HTMLElement>): void {
    event.preventDefault()
    setTargeted(false)
    const dragged = event.dataTransfer.getData('text/plain')
    if (dragged !== '') onDropCell(dragged, lane.key, cell.status)
  }

  return (
    <section
      className="board__cell"
      aria-label={`${lane.title}, ${cell.label}`}
      data-lane={lane.key ?? ''}
      data-status={cell.status}
      data-targeted={targeted || undefined}
      onDragOver={(event) => {
        event.preventDefault()
        event.dataTransfer.dropEffect = 'move'
        setTargeted(true)
      }}
      onDragLeave={() => setTargeted(false)}
      onDrop={drop}
    >
      <ul className="board__tickets">
        {cell.tickets.map((ticket) => (
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
