import type { BacklinkView } from '../wire/BacklinkView'
import type { RecoveryView } from '../wire/RecoveryView'
import type { TicketSummary } from '../wire/TicketSummary'

import { recoverableLabel, ticketHref } from '../shell/bikeshed'

export type NoteRailProps = {
  backlinks: BacklinkView[]
  linked: TicketSummary[]
  recoveries: RecoveryView[]
  onOpenNote: (path: string) => void
}

function name(path: string): string {
  return (path.split('/').pop() ?? path).replace(/\.md$/i, '')
}

function ticketName(ticket: TicketSummary): string {
  return ticket.ref ?? `#${ticket.number ?? '?'}`
}

/**
 * The note's lower dock: who links here, the tickets that name this note, and
 * any draft a killed save left.
 *
 * The section names are the Swift panel's ("Backlinks", the health panel's
 * sibling under the tree); a recovery is drawn as what it is, a draft, never as
 * an error banner.
 */
export function NoteRail({ backlinks, linked, recoveries, onOpenNote }: NoteRailProps) {
  return (
    <section className="rail" aria-label="Note links">
      <div className="rail__section">
        <header className="rail__header">
          <h2 className="rail__title">Backlinks</h2>
          <span className="rail__count">{backlinks.length}</span>
        </header>
        {backlinks.length === 0 ? (
          <p className="rail__empty">Nothing links here yet.</p>
        ) : (
          <ul className="rail__rows">
            {backlinks.map((backlink) => (
              <li key={backlink.path}>
                <button
                  type="button"
                  className="rail__row"
                  onClick={() => onOpenNote(backlink.path)}
                >
                  <span className="rail__row-name">{name(backlink.path)}</span>
                  <span className="rail__row-detail">{backlink.line}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {linked.length === 0 ? null : (
        <div className="rail__section">
          <header className="rail__header">
            <h2 className="rail__title">Tickets</h2>
            <span className="rail__count">{linked.length}</span>
          </header>
          <ul className="rail__rows">
            {linked.map((ticket) => (
              <li key={ticket.id}>
                <a className="rail__row" href={ticketHref(ticket.id)}>
                  <span className="rail__row-name">{ticketName(ticket)}</span>
                  <span className="rail__row-detail">{ticket.title}</span>
                </a>
              </li>
            ))}
          </ul>
        </div>
      )}

      {recoveries.length === 0 ? null : (
        <div className="rail__section rail__section--recovery">
          <header className="rail__header">
            <h2 className="rail__title">Recoveries</h2>
            <span className="rail__count">{recoveries.length}</span>
          </header>
          <ul className="rail__rows">
            {recoveries.map((recovery) => (
              <li key={recovery.file} className="rail__recovery">
                {recoverableLabel(recovery)}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  )
}
