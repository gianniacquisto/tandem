import { Face } from './Face'

export type TicketsFaceProps = {
  revision: number | null
  count: number
}

/**
 * The Tickets face's floor: the store's own revision and size, from the one
 * listing the shell reads. The list, the board and the ticket sheet are PRT-13.
 */
export function TicketsFace({ revision, count }: TicketsFaceProps) {
  const noun = count === 1 ? 'ticket' : 'tickets'

  return (
    <Face title="Tickets" blurb="The board, its lanes and the ticket sheet.">
      <p className="face__fact">
        {revision === null
          ? 'Waiting for the store.'
          : `The store holds ${count} ${noun} at revision ${revision}.`}
      </p>
    </Face>
  )
}
