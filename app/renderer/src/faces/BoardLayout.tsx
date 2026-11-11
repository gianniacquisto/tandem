import { useRef, useState, type DragEvent, type Ref } from 'react'

import type { BoardColumn } from '../shell/board'
import type { TicketSummary } from '../wire/TicketSummary'
import { BoardCard } from './BoardCard'

export type BoardLayoutProps = {
  /** One column per workflow stage, in stage order. */
  columns: BoardColumn[]
  selected: string | null
  onSelect: (id: string) => void
  onOpen: (ticket: TicketSummary) => void
  /** Set the lane filter from a card's chip. */
  onLane: (lane: string) => void
  /** A drop on a column: the dragged id and the stage it landed in. */
  onDropOn: (draggedId: string, status: string) => void
  /** Write a ticket into the column's stage; the composer stays open when it lands. */
  onAdd: (status: string, title: string) => Promise<boolean>
  /** The first column's add control, so the add shortcut reaches it. */
  firstAddRef?: Ref<HTMLButtonElement>
}

/**
 * The board: one column per stage of the active workflow, in stage order, Done
 * included. Each column is its own scroll pane with a sticky head naming the
 * stage and the number of cards in it, and its foot carries the column's add.
 *
 * A card is the lane chip, the title and its footer. A drop on a column is one
 * `ticket.set` carrying that column's stage; a drop in the column the card
 * already stands in writes nothing, because the store's order is per lane and a
 * lane-agnostic column has no order to write.
 */
export function BoardLayout({
  columns,
  selected,
  onSelect,
  onOpen,
  onLane,
  onDropOn,
  onAdd,
  firstAddRef,
}: BoardLayoutProps) {
  return (
    <div className="board">
      {columns.map((column, index) => (
        <Column
          key={column.status}
          column={column}
          selected={selected}
          onSelect={onSelect}
          onOpen={onOpen}
          onLane={onLane}
          onDropOn={onDropOn}
          onAdd={onAdd}
          addRef={index === 0 ? firstAddRef : undefined}
        />
      ))}
    </div>
  )
}

function Column({
  column,
  selected,
  onSelect,
  onOpen,
  onLane,
  onDropOn,
  onAdd,
  addRef,
}: {
  column: BoardColumn
  addRef?: Ref<HTMLButtonElement>
} & Omit<BoardLayoutProps, 'columns' | 'firstAddRef'>) {
  const [targeted, setTargeted] = useState(false)

  function drop(event: DragEvent<HTMLElement>): void {
    event.preventDefault()
    setTargeted(false)
    const dragged = event.dataTransfer.getData('text/plain')
    if (dragged !== '') onDropOn(dragged, column.status)
  }

  return (
    <section
      className="board__column"
      data-status={column.status}
      data-targeted={targeted || undefined}
      aria-label={column.label}
      onDragOver={(event) => {
        event.preventDefault()
        event.dataTransfer.dropEffect = 'move'
        setTargeted(true)
      }}
      onDragLeave={() => setTargeted(false)}
      onDrop={drop}
    >
      <header className="board__column-head">
        <h2 className="board__column-title">
          <span className="board__column-label">{column.label}</span>
          <span className="board__column-count">{column.tickets.length}</span>
        </h2>
      </header>
      <ul className="board__cards">
        {column.tickets.map((ticket) => (
          <BoardCard
            key={ticket.id}
            ticket={ticket}
            selected={ticket.id === selected}
            onSelect={onSelect}
            onOpen={onOpen}
            onLane={onLane}
          />
        ))}
      </ul>
      <ColumnComposer status={column.status} label={column.label} onAdd={onAdd} addRef={addRef} />
    </section>
  )
}

/**
 * The column's foot: "+ Add a card" opens a one-line composer that writes
 * `ticket.add` with the column's stage and keeps itself open and focused for
 * the next title. The write itself is the face's, so the add lands through the
 * one door and the receipt selects the new card.
 */
function ColumnComposer({
  status,
  label,
  onAdd,
  addRef,
}: {
  status: string
  label: string
  onAdd: (status: string, title: string) => Promise<boolean>
  addRef?: Ref<HTMLButtonElement>
}) {
  const [open, setOpen] = useState(false)
  const [title, setTitle] = useState('')
  const input = useRef<HTMLInputElement>(null)

  async function submit(event: { preventDefault: () => void }): Promise<void> {
    event.preventDefault()
    const trimmed = title.trim()
    if (trimmed === '') return
    const landed = await onAdd(status, trimmed)
    if (!landed) return
    setTitle('')
    input.current?.focus()
  }

  if (!open) {
    return (
      <div className="board__add">
        <button
          ref={addRef}
          type="button"
          className="board__add-button"
          onClick={() => setOpen(true)}
        >
          + Add a card
        </button>
      </div>
    )
  }

  return (
    <form
      className="board__add board__add--open"
      onSubmit={(event) => {
        void submit(event)
      }}
    >
      <input
        ref={input}
        className="board__add-input"
        value={title}
        placeholder="Add a card…"
        aria-label={`Add a card to ${label}`}
        autoFocus
        onChange={(event) => setTitle(event.target.value)}
      />
    </form>
  )
}
