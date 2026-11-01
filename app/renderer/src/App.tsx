import { useCallback, useEffect, useReducer, useRef, useState } from 'react'

import type { Call } from '../../src/shared/bridge'
import { BikeshedFace } from './faces/BikeshedFace'
import { SettingsFace } from './faces/SettingsFace'
import { TicketSheet } from './faces/TicketSheet'
import { TicketsFace } from './faces/TicketsFace'
import { UsageFace } from './faces/UsageFace'
import { closeHref, parseRoute, tabHref, type Route } from './shell/route'
import { initialSnapshot, snapshotReducer, type SnapshotState } from './shell/snapshot'
import { refusalBanner, writeBlocked, type WriteCaller, type WriteOutcome } from './shell/write'
import { TABS } from './shell/tabs'
import type { TicketListParams } from './wire/TicketListParams'
import type { TicketSummary } from './wire/TicketSummary'

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
    onWrite: (call: Call, expected: number | null) => Promise<WriteOutcome>
    onDismiss: () => void
    openTicket: string | null
    onOpenTicket: (ticket: TicketSummary) => void
    onPickProject: (lane: string | null) => void
  },
) {
  switch (route.tab) {
    case 'tickets':
      return (
        <TicketsFace
          snapshot={snapshot}
          view={route.view}
          roster={snapshot.roster}
          activeProject={snapshot.activeProject}
          onPickProject={actions.onPickProject}
          onReload={actions.onReload}
          onWrite={actions.onWrite}
          onDismiss={actions.onDismiss}
          openTicket={actions.openTicket}
          onOpenTicket={actions.onOpenTicket}
        />
      )
    case 'bikeshed':
      return (
        <BikeshedFace
          onOpenTicket={actions.onOpenTicket}
          lanes={snapshot.lanes}
          activeProject={snapshot.activeProject}
          onPickProject={actions.onPickProject}
        />
      )
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
  // The sheet's one selection. The hash's `ticket` opens it (a link a person can
  // keep), and a row, a card or a note's ticket row sets it without moving the
  // tab, so Esc leaves the face that opened it exactly where it was.
  const [openTicket, setOpenTicket] = useState<string | null>(
    () => parseRoute(window.location.hash).ticket,
  )
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
      // The whole board, the lanes it is grouped into, the one usage report,
      // and the settings both assignee surfaces read, in one pass: the report is
      // the derivation's (`usage.report`), so the Usage face sums nothing, and
      // the roster is the store's (`settings.get`), so neither surface derives
      // its names from the rows.
      const [board, lanes, usage, settings] = await Promise.all([
        window.tandem.call({ method: 'ticket.list', params: WHOLE_BOARD }),
        window.tandem.call({ method: 'lane.list', params: {} }),
        window.tandem.call({ method: 'usage.report', params: {} }),
        window.tandem.call({ method: 'settings.get', params: {} }),
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
      if ('error' in settings) {
        // A refused settings read leaves both assignee surfaces on the names
        // they already have: Unassigned and `human`. It is not a board failure.
        dispatch({ type: 'notice', message: settings.error.message })
      } else if ('roster' in settings.reply) {
        dispatch({
          type: 'settings',
          roster: settings.reply.roster,
          activeProject: settings.reply.activeProject ?? null,
        })
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
    const followHash = () => {
      const next = parseRoute(window.location.hash)
      setRoute(next)
      setOpenTicket(next.ticket)
    }
    window.addEventListener('hashchange', followHash)
    return () => window.removeEventListener('hashchange', followHash)
  }, [])

  const write = useCallback(
    async (
      call: Call,
      expected: number | null,
      caller: WriteCaller = 'face',
      followUp?: string,
    ): Promise<WriteOutcome> => {
      const refuse = (message: string): WriteOutcome => {
        const words = followUp === undefined ? message : `${followUp} ${message}`
        const banner = refusalBanner(caller, words)
        if (banner !== null) dispatch({ type: 'notice', message: banner })
        return { ok: false, message: words }
      }
      // A follow-up write is authorized by the write that landed before it: the
      // listing moved on purpose, so the revision guard would refuse its own
      // act, and a refusal here names the half that is already stored.
      const blocked = followUp === undefined ? writeBlocked(state.current, expected) : null
      if (blocked !== null) {
        void load()
        return refuse(blocked)
      }
      try {
        const answer = await window.tandem.call(call)
        if ('error' in answer) return refuse(answer.error.message)
        return { ok: true }
      } catch {
        return refuse('The write could not reach the store.')
      }
    },
    [load],
  )

  const faceWrite = useCallback(
    (call: Call, expected: number | null, followUp?: string): Promise<WriteOutcome> =>
      write(call, expected, 'face', followUp),
    [write],
  )

  // The sheet carries its own refusals, so its writes never raise the banner
  // behind the modal.
  const sheetWrite = useCallback(
    (call: Call, expected: number | null): Promise<WriteOutcome> => write(call, expected, 'sheet'),
    [write],
  )

  const dismiss = useCallback(() => dispatch({ type: 'dismiss' }), [])
  const reload = useCallback(() => {
    void load()
  }, [load])
  // Persist the project focus through the shell's one door: the revision guard
  // still applies, so a pick on a stale listing re-reads instead of writing
  // over it, and the change event re-reads the new focus into the snapshot.
  const pickProject = useCallback(
    (lane: string | null) => {
      void faceWrite({ method: 'settings.set', params: { project: lane, roster: null } }, null).then(
        (outcome) => {
          if (outcome.ok) reload()
        },
      )
    },
    [faceWrite, reload],
  )
  const open = useCallback((ticket: TicketSummary) => setOpenTicket(ticket.id), [])
  const closeSheet = useCallback(() => {
    setOpenTicket(null)
    // A sheet opened from a kept link owns the hash's query; closing drops that
    // query and keeps the layout that carried it, so Esc from the board is the
    // board and not the list.
    const current = parseRoute(window.location.hash)
    if (current.ticket !== null) window.location.hash = closeHref(current)
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
        {face(route, snapshot, {
          onReload: reload,
          onWrite: faceWrite,
          onDismiss: dismiss,
          openTicket,
          onOpenTicket: open,
          onPickProject: pickProject,
        })}
        {openTicket === null ? null : (
          <TicketSheet
            ticketId={openTicket}
            lanes={snapshot.lanes}
            roster={snapshot.roster}
            revision={snapshot.revision}
            onWrite={sheetWrite}
            onDismiss={closeSheet}
            onOpenTicket={setOpenTicket}
          />
        )}
      </main>
    </div>
  )
}
