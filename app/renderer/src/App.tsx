import { useCallback, useEffect, useReducer, useRef, useState } from 'react'

import type { Call } from '../../src/shared/bridge'
import { BikeshedFace } from './faces/BikeshedFace'
import { SettingsFace } from './faces/SettingsFace'
import { TicketsFace } from './faces/TicketsFace'
import { UsageFace } from './faces/UsageFace'
import { parseRoute, tabHref, type Route } from './shell/route'
import { initialSnapshot, snapshotReducer, type SnapshotState } from './shell/snapshot'
import { writeBlocked } from './shell/write'
import { TABS } from './shell/tabs'
import type { TicketListParams } from './wire/TicketListParams'

/**
 * The whole store in one listing: no scope, the archive included, every axis
 * absent. The face narrows this snapshot locally, so the stored order the drag
 * arithmetic needs is the same array the rows came from, and the scope counts
 * need no second read.
 */
const WHOLE_BOARD: TicketListParams = {
  filter: 'all',
  text: null,
  status: null,
  assignee: null,
  lane: null,
  archived: true,
  blocked: false,
}

function face(
  route: Route,
  snapshot: SnapshotState,
  actions: {
    onReload: () => void
    onWrite: (call: Call, expected: number | null) => Promise<boolean>
    onDismiss: () => void
  },
) {
  switch (route.tab) {
    case 'tickets':
      return <TicketsFace snapshot={snapshot} view={route.view} {...actions} />
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
 * The shell: header, tab strip, face. It reads the board and the lanes once and
 * again on every change event; faces receive data as props. A write goes through
 * the same door (`window.tandem.call`) and is guarded by the listing's revision:
 * a store that moved under the face answers a visible conflict, never a silent
 * overwrite.
 */
export function App() {
  const [route, setRoute] = useState<Route>(() => parseRoute(window.location.hash))
  const [snapshot, dispatch] = useReducer(snapshotReducer, initialSnapshot)
  const state = useRef(snapshot)
  state.current = snapshot
  const mounted = useRef(true)

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])

  const load = useCallback(async (): Promise<void> => {
    try {
      // The whole board, the lanes it is grouped into, and the one usage
      // report, in one pass: the report is the derivation's (`usage.report`), so
      // the Usage face sums nothing.
      const [board, lanes, usage] = await Promise.all([
        window.tandem.call({ method: 'ticket.list', params: WHOLE_BOARD }),
        window.tandem.call({ method: 'lane.list', params: {} }),
        window.tandem.call({ method: 'usage.report', params: {} }),
      ])
      if (!mounted.current) return
      if ('error' in board) {
        dispatch({ type: 'failed', message: board.error.message })
        return
      }
      if ('error' in lanes) {
        dispatch({ type: 'failed', message: lanes.error.message })
        return
      }
      if (!('lanes' in lanes.reply)) {
        dispatch({ type: 'failed', message: 'The lanes could not be read.' })
        return
      }
      if ('error' in usage) {
        // A refused report leaves the Usage face waiting; it does not take the
        // board down with it.
        dispatch({ type: 'notice', message: usage.error.message })
      } else if ('report' in usage.reply && 'ticketTotal' in usage.reply.report) {
        dispatch({ type: 'usage', report: usage.reply.report })
      }
      if ('tickets' in board.reply && 'revision' in board.reply) {
        dispatch({ type: 'loaded', reply: board.reply, lanes: lanes.reply.lanes })
      }    } catch {
      if (mounted.current) {
        dispatch({ type: 'failed', message: 'The store could not be read.' })
      }
    }
  }, [])

  useEffect(() => {
    void load()
    const stop = window.tandem.onChanged(() => {
      // The event is a hint, never a state: mark the listing stale so a write
      // cannot land on a store the face has not caught up to, then re-read.
      dispatch({ type: 'stale' })
      void load()
    })
    return () => stop()
  }, [load])

  useEffect(() => {
    const followHash = () => setRoute(parseRoute(window.location.hash))
    window.addEventListener('hashchange', followHash)
    return () => window.removeEventListener('hashchange', followHash)
  }, [])

  const write = useCallback(
    async (call: Call, expected: number | null): Promise<boolean> => {
      const blocked = writeBlocked(state.current, expected)
      if (blocked !== null) {
        dispatch({ type: 'notice', message: blocked })
        void load()
        return false
      }
      try {
        const answer = await window.tandem.call(call)
        if ('error' in answer) {
          dispatch({ type: 'notice', message: answer.error.message })
          return false
        }
        return true
      } catch {
        dispatch({ type: 'notice', message: 'The write could not reach the store.' })
        return false
      }
    },
    [load],
  )

  const dismiss = useCallback(() => dispatch({ type: 'dismiss' }), [])
  const reload = useCallback(() => {
    void load()
  }, [load])

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
              aria-selected={entry.id === route.tab}
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
        {snapshot.notice === null ? null : (
          <div className="shell__notice" role="alert">
            <span>{snapshot.notice}</span>
            <button type="button" className="shell__dismiss" onClick={dismiss}>
              Dismiss
            </button>
          </div>
        )}
        {face(route, snapshot, { onReload: reload, onWrite: write, onDismiss: dismiss })}
      </main>
    </div>
  )
}
