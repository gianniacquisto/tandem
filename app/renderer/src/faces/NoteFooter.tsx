import type { BacklinkView } from '../wire/BacklinkView'
import type { RecoveryView } from '../wire/RecoveryView'
import type { TicketSummary } from '../wire/TicketSummary'

import { recoverableLabel, ticketHref } from '../shell/bikeshed'

export type NoteFooterProps = {
  backlinks: BacklinkView[]
  linked: TicketSummary[]
  recoveries: RecoveryView[]
  onOpenNote: (path: string) => void
  /**
   * Opens the ticket sheet over the bikeshed. The row stays an anchor
   * (`ticketHref`): a click opens the sheet where the note is, and a middle
   * click or a copied link still names a ticket a person can come back to.
   */
  onOpenTicket?: (ticket: TicketSummary) => void
}

function name(path: string): string {
  return (path.split('/').pop() ?? path).replace(/\.md$/i, '')
}

function ticketName(ticket: TicketSummary): string {
  return ticket.ref ?? `#${ticket.number ?? '?'}`
}

/**
 * The note's footer: who links here, the tickets that name this note, and any
 * draft a killed save left, after the note in its scrolling page.
 *
 * The section names are the Swift panel's ("Backlinks", the health panel's
 * sibling under the tree); a recovery is drawn as what it is, a draft, never as
 * an error banner. There is no collapse control: a footer costs no width.
 */
export function BacklinksSection({
  backlinks,
  onOpenNote,
}: {
  backlinks: BacklinkView[]
  onOpenNote: (path: string) => void
}) {
  return (
    <div className="note-footer__section">
      <header className="note-footer__header">
        <h2 className="note-footer__title">Backlinks</h2>
        <span className="note-footer__count">{backlinks.length}</span>
      </header>
      {backlinks.length === 0 ? (
        <p className="note-footer__empty">Nothing links here yet.</p>
      ) : (
        <ul className="note-footer__rows">
          {backlinks.map((backlink) => (
            <li key={backlink.path}>
              <button
                type="button"
                className="note-footer__row"
                onClick={() => onOpenNote(backlink.path)}
              >
                <span className="note-footer__row-name">{name(backlink.path)}</span>
                <span className="note-footer__row-detail">{backlink.line}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

export function LinkedTicketsSection({
  linked,
  onOpenTicket,
}: {
  linked: TicketSummary[]
  onOpenTicket?: (ticket: TicketSummary) => void
}) {
  if (linked.length === 0) return null
  return (
    <div className="note-footer__section">
      <header className="note-footer__header">
        <h2 className="note-footer__title">Tickets</h2>
        <span className="note-footer__count">{linked.length}</span>
      </header>
      <ul className="note-footer__rows">
        {linked.map((ticket) => (
          <li key={ticket.id}>
            <a
              className="note-footer__row"
              href={ticketHref(ticket.id)}
              onClick={(event) => {
                if (onOpenTicket === undefined) return
                event.preventDefault()
                onOpenTicket(ticket)
              }}
            >
              <span className="note-footer__row-name">{ticketName(ticket)}</span>
              <span className="note-footer__row-detail">{ticket.title}</span>
            </a>
          </li>
        ))}
      </ul>
    </div>
  )
}

export function RecoveriesSection({ recoveries }: { recoveries: RecoveryView[] }) {
  if (recoveries.length === 0) return null
  return (
    <div className="note-footer__section note-footer__section--recovery">
      <header className="note-footer__header">
        <h2 className="note-footer__title">Recoveries</h2>
        <span className="note-footer__count">{recoveries.length}</span>
      </header>
      <ul className="note-footer__rows">
        {recoveries.map((recovery) => (
          <li key={recovery.file} className="note-footer__recovery">
            {recoverableLabel(recovery)}
          </li>
        ))}
      </ul>
    </div>
  )
}

export function NoteFooter({
  backlinks,
  linked,
  recoveries,
  onOpenNote,
  onOpenTicket,
}: NoteFooterProps) {
  return (
    <footer className="note-footer" aria-label="Note links">
      <BacklinksSection backlinks={backlinks} onOpenNote={onOpenNote} />
      <LinkedTicketsSection linked={linked} onOpenTicket={onOpenTicket} />
      <RecoveriesSection recoveries={recoveries} />
    </footer>
  )
}
