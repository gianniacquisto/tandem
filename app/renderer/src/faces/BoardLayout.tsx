import { boardStatuses, type LaneRow } from '../shell/board'
import type { TicketSummary } from '../wire/TicketSummary'
import { BoardCell } from './BoardCell'

export type BoardLayoutProps = {
  rows: LaneRow[]
  selected: string | null
  onSelect: (id: string) => void
  onToggle: (ticket: TicketSummary) => void
  onOpen?: (ticket: TicketSummary) => void
  onDropOn: (draggedId: string, anchorId: string, after: boolean) => void
  onDropCell: (draggedId: string, lane: string | null, status: string) => void
}

/**
 * The board grid: lanes are rows and the six statuses are columns, `on-hold`
 * parked last, so the workflow reads left to right inside each lane.
 *
 * A drop on a card reorders inside its cell; a drop on the cell itself files
 * the ticket there. The cell's own state change goes through `ticket.set` and
 * its lane change through `ticket.move`, both verbs the list already uses, so
 * the two layouts cannot drift apart.
 */
export function BoardLayout({
  rows,
  selected,
  onSelect,
  onToggle,
  onOpen,
  onDropOn,
  onDropCell,
}: BoardLayoutProps) {
  return (
    <div className="board">
      <div className="board__lane board__lane--head">
        <span className="board__title board__title--head" aria-hidden="true" />
        {boardStatuses().map((status) => (
          <h2 key={status.value} className="board__status" data-status={status.value}>
            {status.label}
          </h2>
        ))}
      </div>
      {rows.map((row) => (
        <div
          className="board__lane"
          key={row.key ?? 'uncategorized'}
          data-lane={row.key ?? ''}
        >
          <h2 className="board__title">
            <span>{row.title}</span>
            <span className="board__count">
              {row.cells.reduce((count, cell) => count + cell.tickets.length, 0)}
            </span>
          </h2>
          {row.cells.map((cell) => (
            <BoardCell
              key={cell.status}
              lane={row}
              cell={cell}
              selected={selected}
              onSelect={onSelect}
              onToggle={onToggle}
              onOpen={onOpen}
              onDropOn={onDropOn}
              onDropCell={onDropCell}
            />
          ))}
        </div>
      ))}
    </div>
  )
}
