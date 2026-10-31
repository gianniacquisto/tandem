import type { LaneColumn } from '../shell/board'
import type { TicketSummary } from '../wire/TicketSummary'
import { BoardColumn } from './BoardColumn'

export type BoardLayoutProps = {
  columns: LaneColumn[]
  selected: string | null
  onSelect: (id: string) => void
  onToggle: (ticket: TicketSummary) => void
  onOpen?: (ticket: TicketSummary) => void
  onDropOn: (draggedId: string, anchorId: string, after: boolean) => void
  onDropInto: (draggedId: string, lane: string | null) => void
}

/**
 * The board layout: one column per lane, the same rows set on cards. A drop on a
 * card reorders inside that lane; a drop anywhere else in the column files the
 * ticket at that lane's end. Both writes are `ticket.move`, the verb the list
 * uses, so the two layouts cannot drift apart.
 */
export function BoardLayout({
  columns,
  selected,
  onSelect,
  onToggle,
  onOpen,
  onDropOn,
  onDropInto,
}: BoardLayoutProps) {
  return (
    <div className="board">
      {columns.map((column) => (
        <BoardColumn
          key={column.key ?? 'uncategorized'}
          column={column}
          selected={selected}
          onSelect={onSelect}
          onToggle={onToggle}
          onOpen={onOpen}
          onDropOn={onDropOn}
          onDropInto={onDropInto}
        />
      ))}
    </div>
  )
}
