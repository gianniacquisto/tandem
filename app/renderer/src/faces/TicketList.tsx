import type { LaneColumn } from '../shell/board'
import type { TicketSummary } from '../wire/TicketSummary'
import { TicketRow } from './TicketRow'

export type TicketListProps = {
  columns: LaneColumn[]
  selected: string | null
  onSelect: (id: string) => void
  onToggle: (ticket: TicketSummary) => void
  onOpen?: (ticket: TicketSummary) => void
  onDropOn: (draggedId: string, anchorId: string, after: boolean) => void
}

/**
 * The list layout: the default area first with no header, then each named lane
 * under its own, every row in the store's order. It is the board's same rows,
 * cut flat.
 */
export function TicketList({
  columns,
  selected,
  onSelect,
  onToggle,
  onOpen,
  onDropOn,
}: TicketListProps) {
  return (
    <div className="list">
      {columns.map((column) => (
        <section className="list__lane" key={column.key ?? 'uncategorized'}>
          {column.key === null ? null : (
            <h2 className="list__lane-title">
              <span>{column.title}</span>
              <span className="list__lane-count">{column.tickets.length}</span>
            </h2>
          )}
          <ul className="list__rows">
            {column.tickets.map((ticket) => (
              <TicketRow
                key={ticket.id}
                ticket={ticket}
                layout="list"
                selected={ticket.id === selected}
                onSelect={() => onSelect(ticket.id)}
                onToggle={() => onToggle(ticket)}
                onOpen={onOpen === undefined ? undefined : () => onOpen(ticket)}
                onDropOn={onDropOn}
              />
            ))}
          </ul>
        </section>
      ))}
    </div>
  )
}
