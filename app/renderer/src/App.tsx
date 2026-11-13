import { useCallback, useEffect, useReducer, useRef, useState } from 'react'

import type { Method } from '../../src/shared/bridge'
import { BikeshedFace } from './faces/BikeshedFace'
import { SheetBoundary } from './faces/SheetBoundary'
import { TicketSheet } from './faces/TicketSheet'
import { TicketsFace } from './faces/TicketsFace'
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
    onWrite: (call: Method, expected: number | null) => Promise<WriteOutcome>
    onDismiss: () => void
    openTicket: string | null
    onOpenTicket: (ticket: { id: string }, focus?: 'description' | null) => void
    activeProject: string | null
    onPickProject: (lane: string | null) => void
  },
) {
  switch (route.tab) {
    case 'tickets':
      return (
        <TicketsFace
          snapshot={snapshot}
          view={route.view}
          activeProject={actions.activeProject}
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
          projects={snapshot.projects}
          activeProject={actions.activeProject}
          onPickProject={actions.onPickProject}
        />
      )
  }
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
  // Where a programmatic open lands its focus: the description composer for a
  // quick-add handoff, else the dialog. Cleared with the sheet, so a kept link
  // never inherits the handoff.
  const [sheetFocus, setSheetFocus] = useState<'description' | null>(null)
  // The project focus is the shell's own: no store row carries it any more.
  const [activeProject, setActiveProject] = useState<string | null>(null)
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
      // The whole board, the lanes it is grouped into, and the projects those
      // lanes belong to, in one pass. The board snapshot stays lane-keyed; the
      // renderer joins `project.list` for the mapping instead of growing the
      // snapshot.
      const [board, lanes, projects] = await Promise.all([
        window.tandem.call({ method: 'ticket.list', params: WHOLE_BOARD }),
        window.tandem.call({ method: 'lane.list', params: {} }),
        window.tandem.call({ method: 'project.list', params: {} }),
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
      // A refused project listing leaves the board ungrouped rather than
      // taking it down: the lanes still draw, flat, in lane order.
      const projectRows =
        !('error' in projects) && 'projects' in projects.reply
          ? projects.reply.projects
          : []
      if ('error' in projects) {
        dispatch({ type: 'notice', message: projects.error.message })
      }
      if ('tickets' in board.reply && 'revision' in board.reply) {
        dispatch({
          type: 'loaded',
          reply: board.reply,
          lanes: lanes.reply.lanes,
          projects: projectRows,
        })
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
      call: Method,
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
        return { ok: true, reply: answer.reply }
      } catch {
        return refuse('The write could not reach the store.')
      }
    },
    [load],
  )

  const faceWrite = useCallback(
    (call: Method, expected: number | null, followUp?: string): Promise<WriteOutcome> =>
      write(call, expected, 'face', followUp),
    [write],
  )

  // The sheet carries its own refusals, so its writes never raise the banner
  // behind the modal.
  const sheetWrite = useCallback(
    (call: Method, expected: number | null, followUp?: string): Promise<WriteOutcome> =>
      write(call, expected, 'sheet', followUp),
    [write],
  )

  const dismiss = useCallback(() => dispatch({ type: 'dismiss' }), [])
  const reload = useCallback(() => {
    void load()
  }, [load])
  // The focus is client state now: no store write, no revision guard, and no
  // change event to wait for.
  const pickProject = useCallback((lane: string | null) => {
    setActiveProject(lane)
  }, [])
  const open = useCallback(
    (ticket: { id: string }, focus?: 'description' | null) => {
      setOpenTicket(ticket.id)
      setSheetFocus(focus ?? null)
    },
    [],
  )
  const closeSheet = useCallback(() => {
    setOpenTicket(null)
    setSheetFocus(null)
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
      </header>

      <main className={route.tab === 'bikeshed' ? 'shell__body shell__body--bleed' : 'shell__body'}>
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
          activeProject,
          onPickProject: pickProject,
        })}
        {openTicket === null ? null : (
          <SheetBoundary key={openTicket} onDismiss={closeSheet}>
            <TicketSheet
              ticketId={openTicket}
              lanes={snapshot.lanes}
              revision={snapshot.revision}
              onWrite={sheetWrite}
              onDismiss={closeSheet}
              onOpenTicket={setOpenTicket}
              initialFocus={sheetFocus}
            />
          </SheetBoundary>
        )}
      </main>
    </div>
  )
}
