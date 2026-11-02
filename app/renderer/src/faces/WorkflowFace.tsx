import { useEffect, useState } from 'react'

import type { LaneDeletionReply } from '../wire/LaneDeletionReply'
import type { LaneListReply } from '../wire/LaneListReply'
import type { Reply } from '../wire/Reply'
import type { Sprint } from '../wire/Sprint'
import type { SprintListReply } from '../wire/SprintListReply'
import type { SprintMode } from '../wire/SprintMode'
import type { SprintReply } from '../wire/SprintReply'
import type { TicketSummary } from '../wire/TicketSummary'
import { tabHref } from '../shell/route'
import { EmptyState } from './EmptyState'
import { Face } from './Face'
import {
  countsByPass,
  DAG_HEIGHT,
  DAG_WIDTH,
  edgeHandoffs,
  edgePath,
  nodeFrame,
  PASSES,
  positionBatch,
  type PassId,
} from './workflow'

export type WorkflowFaceProps = {
  /** The whole board (`ticket.list`), so batch tickets read their status. */
  tickets: TicketSummary[]
  /** The lanes the shell read, first paint before the face's own read lands. */
  lanes: string[]
  /** The lane the faces draw, or null for every lane. */
  activeProject: string | null
  /** Opens a ticket's sheet; a batch chip is inert until the sheet exists. */
  onOpenTicket?: (ticket: TicketSummary) => void
}

/** The lane the graph draws when nothing focuses it: the focus, else the first. */
function pickLane(lanes: string[], activeProject: string | null): string {
  if (activeProject !== null && lanes.includes(activeProject)) return activeProject
  return lanes[0] ?? ''
}

function isSprintReply(reply: Reply): reply is SprintReply {
  return 'revision' in reply && 'sprint' in reply
}

function isSprintListReply(reply: Reply): reply is SprintListReply {
  return 'revision' in reply && 'sprints' in reply
}

function isLaneListReply(reply: Reply): reply is LaneListReply {
  return 'revision' in reply && 'lanes' in reply
}

function isLaneDeletionReply(reply: Reply): reply is LaneDeletionReply {
  return 'revision' in reply && 'lanes' in reply && 'moved' in reply
}

/** One lane's workflow read: the open record and how many the lane holds open. */
type LaneSprint =
  | { ok: true; sprint: Sprint | null; note: string | null; openCount: number }
  | { ok: false; failure: string }

async function readLaneSprint(lane: string): Promise<LaneSprint> {
  const [show, listed] = await Promise.all([
    window.tandem.call({ method: 'sprint.show', params: { lane } }),
    window.tandem.call({ method: 'sprint.list', params: { lane, status: 'open' } }),
  ])
  if ('error' in listed) return { ok: false, failure: listed.error.message }
  if (!isSprintListReply(listed.reply)) {
    return { ok: false, failure: 'The sprint records were not readable.' }
  }
  const openCount = listed.reply.sprints.length
  if ('error' in show) {
    // No open record is the drawn empty state, not a failure: the list just
    // said the lane holds none. Any other refusal is the error state.
    if (openCount === 0) return { ok: true, sprint: null, note: null, openCount }
    return { ok: false, failure: show.error.message }
  }
  if (!isSprintReply(show.reply)) {
    return { ok: false, failure: 'The open sprint was not readable.' }
  }
  return { ok: true, sprint: show.reply.sprint, note: show.reply.note ?? null, openCount }
}

/** A batch chip's words: the ref and title, or the id when the board lost it. */
function chipLabel(ticket: TicketSummary | null, id: string): string {
  if (ticket === null) return `${id}: not on the board`
  const ref = ticket.ref ?? ticket.id
  return `${ref}: ${ticket.title}`
}

/**
 * The Workflow face: the sprint loop drawn as a DAG, configured through the
 * app. The passes are the nodes, the handoffs the edges, and the open sprint's
 * batch tickets sit at the node their status earns (`workflow.ts`).
 *
 * The face draws the wire's answers and computes no store truth: the nodes and
 * the tallies come from `sprint.show` plus `ticket.list`, the mode picker opens
 * the sprint through `sprint.open`, the lanes read and write through `lane.*`,
 * and the host setup stays one link into the Settings face. ADR 0032 names this
 * face; TAN-254 carries the wire it stands on.
 */
export function WorkflowFace({ tickets, lanes, activeProject, onOpenTicket }: WorkflowFaceProps) {
  const [known, setKnown] = useState<string[]>(lanes)
  const [lane, setLane] = useState<string>(() => pickLane(lanes, activeProject))
  const [sprint, setSprint] = useState<Sprint | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [openCount, setOpenCount] = useState<number | null>(null)
  const [mode, setMode] = useState<SprintMode>('serial')
  const [failure, setFailure] = useState<string | null>(null)
  const [working, setWorking] = useState(false)
  const [newLane, setNewLane] = useState('')
  const [renameTo, setRenameTo] = useState('')

  useEffect(() => {
    if (lane === '' || !known.includes(lane)) {
      const next = pickLane(known, activeProject)
      if (next !== lane) setLane(next)
    }
  }, [known, activeProject, lane])

  useEffect(() => {
    if (lane === '') {
      setSprint(null)
      setNote(null)
      setOpenCount(null)
      return
    }
    let live = true

    async function read(): Promise<void> {
      const answer = await readLaneSprint(lane)
      if (!live) return
      if (!answer.ok) {
        setFailure(answer.failure)
        return
      }
      setSprint(answer.sprint)
      if (answer.sprint !== null) setMode(answer.sprint.mode)
      setNote(answer.note)
      setOpenCount(answer.openCount)
      setFailure(null)
    }

    void read()
    return () => {
      live = false
    }
  }, [lane])

  // The face's own lane read: the shell's listing is first paint, and a write's
  // receipt redraws the list below, so this is the freshness between the two.
  useEffect(() => {
    let live = true

    async function readLanes(): Promise<void> {
      const answer = await window.tandem.call({ method: 'lane.list', params: {} })
      if (!live) return
      if ('error' in answer) return
      if (isLaneListReply(answer.reply)) setKnown(answer.reply.lanes)
    }

    void readLanes()
    return () => {
      live = false
    }
  }, [])

  async function refreshSprint(): Promise<void> {
    if (lane === '') return
    setWorking(true)
    try {
      const answer = await readLaneSprint(lane)
      if (!answer.ok) {
        setFailure(answer.failure)
        return
      }
      setSprint(answer.sprint)
      if (answer.sprint !== null) setMode(answer.sprint.mode)
      setNote(answer.note)
      setOpenCount(answer.openCount)
      setFailure(null)
    } finally {
      setWorking(false)
    }
  }

  // The one sprint write: `sprint.open` at the picker's mode. A second open on
  // a lane whose sprint is open returns that record unchanged with a note, so
  // re-running the opener cannot fork the sprint.
  async function openSprint(): Promise<void> {
    if (lane === '') return
    setWorking(true)
    try {
      const opened = await window.tandem.call({
        method: 'sprint.open',
        params: { lane, mode, session: null },
      })
      if ('error' in opened) {
        setFailure(opened.error.message)
        return
      }
      if (!isSprintReply(opened.reply)) {
        setFailure('The opened sprint was not readable.')
        return
      }
      setSprint(opened.reply.sprint)
      setNote(opened.reply.note ?? null)
      setOpenCount(1)
      setFailure(null)
    } finally {
      setWorking(false)
    }
  }

  async function addLane(): Promise<void> {
    const name = newLane.trim()
    if (name === '') return
    const written = await window.tandem.call({
      method: 'lane.add',
      params: { name, abbr: null },
    })
    if ('error' in written) {
      setFailure(written.error.message)
      return
    }
    if (!isLaneListReply(written.reply)) {
      setFailure('The lane was not writable.')
      return
    }
    setKnown(written.reply.lanes)
    setLane(name)
    setNewLane('')
    setFailure(null)
  }

  async function renameLane(): Promise<void> {
    const to = renameTo.trim()
    if (lane === '' || to === '') return
    const written = await window.tandem.call({
      method: 'lane.rename',
      params: { from: lane, to, abbr: null },
    })
    if ('error' in written) {
      setFailure(written.error.message)
      return
    }
    if (!isLaneListReply(written.reply)) {
      setFailure('The lane was not writable.')
      return
    }
    setKnown(written.reply.lanes)
    setLane(to)
    setRenameTo('')
    setFailure(null)
  }

  async function deleteLane(): Promise<void> {
    if (lane === '') return
    const written = await window.tandem.call({ method: 'lane.delete', params: { name: lane } })
    if ('error' in written) {
      setFailure(written.error.message)
      return
    }
    if (!isLaneDeletionReply(written.reply)) {
      setFailure('The lane was not writable.')
      return
    }
    setKnown(written.reply.lanes)
    setLane(pickLane(written.reply.lanes, activeProject))
    setFailure(null)
  }

  const placed = sprint === null ? [] : positionBatch(sprint.batch, tickets)
  const counts = countsByPass(placed)
  const summary = placedSummary(sprint, counts)

  return (
    <Face
      wide
      title="Workflow"
      blurb="The sprint loop as a DAG: the passes, the handoffs, and the open sprint's batch on the graph."
    >
      <div className="workflow">
        <section className="workflow__section" aria-label="Lane">
          <h2 className="workflow__heading">Lane</h2>
          {known.length === 0 ? (
            <p className="workflow__fact">No lanes yet: name the first below.</p>
          ) : (
            <label className="workflow__row">
              <span className="workflow__label">The sprint loop runs in</span>
              <select
                className="workflow__select"
                value={lane}
                onChange={(event) => setLane(event.target.value)}
                aria-label="The lane whose sprint the graph draws"
              >
                {known.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
          )}
          <form
            className="workflow__row"
            onSubmit={(event) => {
              event.preventDefault()
              void addLane()
            }}
          >
            <input
              className="workflow__input"
              type="text"
              value={newLane}
              placeholder="A lane to add"
              aria-label="A lane to add"
              onChange={(event) => setNewLane(event.target.value)}
            />
            <button type="submit" className="workflow__action">
              Add lane
            </button>
          </form>
          {lane === '' ? null : (
            <div className="workflow__row">
              <form
                className="workflow__rename"
                onSubmit={(event) => {
                  event.preventDefault()
                  void renameLane()
                }}
              >
                <input
                  className="workflow__input"
                  type="text"
                  value={renameTo}
                  placeholder={`Rename ${lane} to…`}
                  aria-label={`Rename ${lane} to`}
                  onChange={(event) => setRenameTo(event.target.value)}
                />
                <button type="submit" className="workflow__action">
                  Rename
                </button>
              </form>
              <button type="button" className="workflow__action" onClick={() => void deleteLane()}>
                Delete {lane}
              </button>
            </div>
          )}
          <p className="workflow__hint">
            Runs `tandem-cli lane add`, `lane rename` and `lane delete`: a delete moves the
            lane&apos;s tickets to the default area, never away.
          </p>
        </section>

        {openCount === null && sprint === null && failure === null ? (
          <p className="face__fact">Waiting for the store.</p>
        ) : null}

        {failure === null && sprint !== null ? (
          <section className="workflow__section" aria-label="Open sprint">
            <h2 className="workflow__heading">
              Sprint {sprint.number} in {sprint.lane}
            </h2>
            <p className="workflow__meta">
              <span className="workflow__mode">{sprint.mode}</span>
              <span>Opened {sprint.openedAt}</span>
              {sprint.session === null ? null : <span>Session {sprint.session}</span>}
            </p>
            {note === null ? null : <p className="workflow__note">{note}</p>}
            <svg
              className="workflow__dag"
              viewBox={`0 0 ${DAG_WIDTH} ${DAG_HEIGHT}`}
              role="img"
              aria-label={summary}
            >
              <defs>
                <marker
                  id="workflow-arrow"
                  viewBox="0 0 8 8"
                  refX="7"
                  refY="4"
                  markerWidth="8"
                  markerHeight="8"
                  orient="auto-start-reverse"
                >
                  <path d="M0,0 L8,4 L0,8 z" className="workflow__arrowhead" />
                </marker>
              </defs>
              {edgeHandoffs().map(([from, to]) => {
                const fromIndex = PASSES.findIndex((pass) => pass.id === from)
                const toIndex = PASSES.findIndex((pass) => pass.id === to)
                return (
                  <path
                    key={`${from}-${to}`}
                    d={edgePath(nodeFrame(fromIndex), nodeFrame(toIndex))}
                    className="workflow__edge"
                    markerEnd="url(#workflow-arrow)"
                  />
                )
              })}
              {PASSES.map((pass, index) => {
                const frame = nodeFrame(index)
                return (
                  <g key={pass.id}>
                    <rect
                      x={frame.x}
                      y={frame.y}
                      width={frame.width}
                      height={frame.height}
                      rx={9}
                      className="workflow__node"
                    />
                    <text
                      x={frame.x + frame.width / 2}
                      y={frame.y + 24}
                      textAnchor="middle"
                      className="workflow__node-label"
                    >
                      {pass.label}
                    </text>
                    <text
                      x={frame.x + frame.width / 2}
                      y={frame.y + 42}
                      textAnchor="middle"
                      className="workflow__node-count"
                    >
                      {ticketNoun(counts[pass.id])}
                    </text>
                  </g>
                )
              })}
            </svg>
            {placed.length === 0 ? (
              <p className="workflow__fact">The batch is empty: no tickets drawn yet.</p>
            ) : (
              <ul className="workflow__batch">
                {PASSES.map((pass) => {
                  const at = placed.filter((one) => one.pass === pass.id)
                  if (at.length === 0) return null
                  return (
                    <li key={pass.id} className="workflow__node-batch">
                      <span className="workflow__node-name">{pass.label}</span>
                      <span className="workflow__chips">
                        {at.map((one) => {
                          if (one.ticket === null) {
                            return (
                              <span key={one.id} className="workflow__chip workflow__chip--gone">
                                {chipLabel(null, one.id)}
                              </span>
                            )
                          }
                          const ticket = one.ticket
                          return (
                            <button
                              key={one.id}
                              type="button"
                              className="workflow__chip"
                              onClick={() => onOpenTicket?.(ticket)}
                              title={`Open ${chipLabel(ticket, one.id)}`}
                            >
                              {chipLabel(ticket, one.id)}
                            </button>
                          )
                        })}
                      </span>
                    </li>
                  )
                })}
              </ul>
            )}
            {sprint.runs.length === 0 ? (
              <p className="workflow__fact">No runs yet: the build records each landing here.</p>
            ) : (
              <ol className="workflow__runs">
                {sprint.runs.map((run, index) => (
                  <li key={`${index}-${run}`} className="workflow__run">
                    {run}
                  </li>
                ))}
              </ol>
            )}
          </section>
        ) : null}

        {failure === null && sprint === null && openCount !== null && lane !== '' ? (
          <EmptyState
            title={`No sprint is open in ${lane}`}
            subtitle="Pick how the batch runs, then open the sprint: the loop draws here."
          >
            <form
              className="workflow__opener"
              onSubmit={(event) => {
                event.preventDefault()
                void openSprint()
              }}
            >
              <div className="workflow__picker" role="radiogroup" aria-label="Sprint mode">
                {(['serial', 'parallel'] as const).map((candidate) => (
                  <label key={candidate} className="workflow__mode-option">
                    <input
                      type="radio"
                      name="sprint-mode"
                      value={candidate}
                      checked={mode === candidate}
                      onChange={() => setMode(candidate)}
                    />
                    {candidate === 'serial' ? 'Serial: one ticket at a time' : 'Parallel: the batch at once'}
                  </label>
                ))}
              </div>
              <button type="submit" className="workflow__action" disabled={working}>
                {working ? 'Opening…' : `Open sprint in ${lane}`}
              </button>
            </form>
            <p className="workflow__hint">
              Runs `tandem-cli sprint open --lane {lane} --mode {mode}`: the record carries the
              batch, the mode and the run log.
            </p>
          </EmptyState>
        ) : null}

        {failure === null ? null : (
          <div className="workflow__section">
            <p className="workflow__error" role="alert">
              {failure}
            </p>
            <button
              type="button"
              className="workflow__action"
              onClick={() => void refreshSprint()}
              disabled={lane === '' || working}
            >
              Try again
            </button>
          </div>
        )}

        <p className="workflow__hint">
          The machine behind the loop lives on the <a href={tabHref('settings')}>Settings face</a>:
          its hosts and checks, not a second copy here.
        </p>
      </div>
    </Face>
  )
}

/** The DAG's spoken summary: the sprint and the tallies, for the graph's label. */
function placedSummary(sprint: Sprint | null, counts: Record<PassId, number>): string {
  if (sprint === null) return 'No open sprint.'
  const tallies = PASSES.map((pass) => `${counts[pass.id]} at ${pass.label.toLowerCase()}`)
  return `Sprint ${sprint.number} in ${sprint.lane}, ${sprint.mode}: ${tallies.join(', ')}.`
}

/** A node's tally: "1 ticket", "8 tickets", "no tickets". */
function ticketNoun(count: number): string {
  if (count === 0) return 'no tickets'
  return count === 1 ? '1 ticket' : `${count} tickets`
}
