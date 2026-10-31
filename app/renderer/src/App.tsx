import { useEffect, useReducer, useState } from 'react'

import { BikeshedFace } from './faces/BikeshedFace'
import { SettingsFace } from './faces/SettingsFace'
import { TicketsFace } from './faces/TicketsFace'
import { UsageFace } from './faces/UsageFace'
import { parseRoute, tabHref } from './shell/route'
import { initialSnapshot, snapshotReducer, type SnapshotState } from './shell/snapshot'
import { TABS, type TabId } from './shell/tabs'
import type { TicketListParams } from './wire/TicketListParams'

/** `ticket.list`'s absent flags: the store's whole open board. */
const OPEN_BOARD: TicketListParams = {
  filter: null,
  text: null,
  status: null,
  assignee: null,
  lane: null,
  archived: false,
  blocked: false,
}

function face(tab: TabId, snapshot: SnapshotState) {
  switch (tab) {
    case 'tickets':
      return <TicketsFace revision={snapshot.revision} count={snapshot.tickets} />
    case 'bikeshed':
      return <BikeshedFace />
    case 'usage':
      return <UsageFace report={snapshot.usage} />
    case 'settings':
      return <SettingsFace />
  }
}

function revisionLabel(snapshot: SnapshotState): string {
  if (snapshot.revision !== null) return `rev ${snapshot.revision}`
  return snapshot.status === 'failed' ? 'no store' : 'connecting'
}

/**
 * The shell: header, tab strip, face. It reads the board once and again on
 * every change event; faces receive data as props and call nothing.
 */
export function App() {
  const [tab, setTab] = useState<TabId>(() => parseRoute(window.location.hash))
  const [snapshot, dispatch] = useReducer(snapshotReducer, initialSnapshot)

  useEffect(() => {
    const followHash = () => setTab(parseRoute(window.location.hash))
    window.addEventListener('hashchange', followHash)
    return () => window.removeEventListener('hashchange', followHash)
  }, [])

  useEffect(() => {
    let live = true

    async function load(): Promise<void> {
      try {
        // The board and the one usage report, in one pass: the report is the
        // derivation's (`usage.report`), so the Usage face sums nothing.
        const [board, usage] = await Promise.all([
          window.tandem.call({ method: 'ticket.list', params: OPEN_BOARD }),
          window.tandem.call({ method: 'usage.report', params: {} }),
        ])
        if (!live) return
        if ('error' in board) {
          dispatch({ type: 'failed', message: board.error.message })
          return
        }
        if ('error' in usage) {
          dispatch({ type: 'failed', message: usage.error.message })
          return
        }
        // Two replies carry a `report` (the doctor's and the usage face's), so
        // the narrow names the field only this one has.
        if ('report' in usage.reply && 'ticketTotal' in usage.reply.report) {
          dispatch({ type: 'usage', report: usage.reply.report })
        }
        const reply = board.reply
        if ('tickets' in reply && 'revision' in reply) {
          dispatch({ type: 'loaded', reply })
        }
      } catch {
        if (live) {
          dispatch({ type: 'failed', message: 'The store could not be read.' })
        }
      }
    }

    void load()
    const stop = window.tandem.onChanged(() => {
      void load()
    })

    return () => {
      live = false
      stop()
    }
  }, [])

  return (
    <div className="shell">
      <header className="shell__header">
        <span className="shell__wordmark">Tandem</span>
        <nav className="tabs" aria-label="Tandem">
          {TABS.map((entry) => (
            <a
              key={entry.id}
              className="tab"
              href={tabHref(entry.id)}
              aria-selected={entry.id === tab}
            >
              {entry.label}
            </a>
          ))}
        </nav>
        <span className="shell__revision" aria-live="polite">
          {revisionLabel(snapshot)}
        </span>
      </header>

      <main className="shell__body">
        {snapshot.error === null ? null : (
          <p className="shell__error" role="alert">
            {snapshot.error}
          </p>
        )}
        {face(tab, snapshot)}
      </main>
    </div>
  )
}
