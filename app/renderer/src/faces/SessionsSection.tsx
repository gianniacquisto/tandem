import { useEffect, useState } from 'react'

import {
  isLongOutput,
  outputPreview,
  resolveInitialExpanded,
  sheetSessionHref,
  unavailableNotice,
  usageLine,
} from '../shell/sessions'
import { commentTime } from '../shell/sheet'
import type { Session } from '../wire/Session'
import type { TranscriptBlock } from '../wire/TranscriptBlock'
import type { TranscriptEntry } from '../wire/TranscriptEntry'
import type { Reply } from '../wire/Reply'
import type { JsonValue } from '../wire/serde_json/JsonValue'

export type SessionsSectionProps = {
  /** The ticket whose linked records the section draws: the ticket's id. */
  ticketId: string
  /** The shell's revision: a move re-reads the rows, live or not. */
  revision: number | null
  /** The deep-linked Tandem record id, or null when the link names none. */
  expandedSession: string | null
}

type RowTranscript = {
  entries: TranscriptEntry[]
  nextCursor: number | null
  unavailable: string | null
  loading: boolean
  outputsOpen: string[]
}

function sessionsOf(reply: Reply): Session[] | null {
  return 'sessions' in reply ? reply.sessions : null
}

function statusLabel(record: Session): string {
  if (record.status === 'running') return 'running'
  return `ended (${record.outcome ?? 'unknown'})`
}

function headerLabel(record: Session): string {
  const agent = record.agent === null || record.agent.trim() === '' ? 'a pass' : record.agent
  return `${agent} · ${statusLabel(record)}`
}

function stampLabel(at: string | null): string | null {
  if (at === null) return null
  return commentTime(at)
}

function formatInput(input: JsonValue): string {
  try {
    return JSON.stringify(input, null, 2) ?? 'null'
  } catch {
    return 'null'
  }
}

function ToolOutput({ output, outputKey }: { output: string; outputKey: string }) {
  const [open, setOpen] = useState(false)
  const folded = isLongOutput(output)
  const shown = open || !folded ? output : outputPreview(output)
  return (
    <div className="sheet__session-output">
      <pre className="sheet__session-io">{shown}</pre>
      {folded ? (
        <button
          type="button"
          className="sheet__toggle"
          aria-expanded={open}
          aria-controls={outputKey}
          onClick={() => setOpen((current) => !current)}
        >
          {open ? 'Collapse output' : 'Expand output'}
        </button>
      ) : null}
    </div>
  )
}

function TranscriptBlockView({ block, outputKey }: { block: TranscriptBlock; outputKey: string }) {
  if (block.type === 'text') {
    return <p className="sheet__session-text">{block.text}</p>
  }
  if (block.type === 'reasoning') {
    return <p className="sheet__session-thinking">{block.text}</p>
  }
  return (
    <div className="sheet__session-tool">
      <header className="sheet__session-tool-head">
        <span className="sheet__session-tool-name">{block.name}</span>
        <span className="sheet__session-tool-status">{block.status}</span>
      </header>
      <pre className="sheet__session-io">{formatInput(block.input)}</pre>
      {block.output === null ? null : (
        <ToolOutput output={block.output} outputKey={outputKey} />
      )}
    </div>
  )
}

/**
 * The ticket sheet's Sessions section: every session record linked to the
 * ticket, newest first, with the harness transcript behind each row.
 *
 * The section always renders: linked rows, or the empty state when none are.
 * Rows start collapsed, except the deep-linked one; a transcript loads on
 * expand only, paging with load-more. A running row shows a live marker and
 * re-reads on every store change; an ended row freezes at what it last read. A
 * row the harness no longer holds says so on the row, and the section still
 * renders: one row's failure never takes the section down.
 */
export function SessionsSection({ ticketId, revision, expandedSession }: SessionsSectionProps) {
  const [sessions, setSessions] = useState<Session[] | null>(null)
  const [readError, setReadError] = useState<string | null>(null)
  const [expanded, setExpanded] = useState<string[]>([])
  const [linkNotice, setLinkNotice] = useState<string | null>(null)
  const [transcripts, setTranscripts] = useState<Record<string, RowTranscript>>({})

  // The linked rows, re-read when the sheet opens and on every store change.
  useEffect(() => {
    let live = true
    async function load(): Promise<void> {
      const answer = await window.tandem.call({ method: 'session.list', params: { ticket: ticketId, status: null } })
      if (!live) return
      if ('error' in answer) {
        setReadError(answer.error.message)
        setSessions(null)
        return
      }
      const rows = sessionsOf(answer.reply) ?? []
      setReadError(null)
      setSessions(rows)
    }
    void load()
    return () => {
      live = false
    }
  }, [ticketId, revision])

  async function readTranscript(id: string, cursor: number | null): Promise<void> {
    if (cursor === null) {
      setTranscripts((current) => ({
        ...current,
        [id]: { entries: [], nextCursor: null, unavailable: null, loading: true, outputsOpen: [] },
      }))
    } else {
      setTranscripts((current) => ({
        ...current,
        [id]: { ...(current[id] ?? { entries: [], nextCursor: null, unavailable: null, outputsOpen: [] }), loading: true },
      }))
    }
    const answer = await window.tandem.call({
      method: 'session.transcript',
      params: { id, cursor, limit: null },
    })
    if ('error' in answer) {
      // A refused read is a row that cannot load, not a section that cannot
      // draw: the reason lands on the row, beside its header.
      setTranscripts((current) => ({
        ...current,
        [id]: {
          entries: cursor === null ? [] : (current[id]?.entries ?? []),
          nextCursor: null,
          unavailable: unavailableNotice(answer.error.message),
          loading: false,
          outputsOpen: current[id]?.outputsOpen ?? [],
        },
      }))
      return
    }
    if (!('entries' in answer.reply)) return
    const page = answer.reply
    setTranscripts((current) => {
      const kept = cursor === null ? [] : (current[id]?.entries ?? [])
      return {
        ...current,
        [id]: {
          entries: [...kept, ...page.entries],
          nextCursor: page.nextCursor,
          unavailable: page.unavailable === null ? null : unavailableNotice(page.unavailable),
          loading: false,
          outputsOpen: current[id]?.outputsOpen ?? [],
        },
      }
    })
  }

  // The deep-linked row expands once its rows have loaded; a sid nothing links
  // is ignored with a notice, never an expansion.
  useEffect(() => {
    if (sessions === null) return
    const resolved = resolveInitialExpanded(
      sessions.map((record) => record.id),
      expandedSession,
    )
    setLinkNotice(resolved.notice)
    if (resolved.expanded !== null) {
      const id = resolved.expanded
      setExpanded((current) => (current.includes(id) ? current : [...current, id]))
      setTranscripts((current) => {
        if (current[id] !== undefined) return current
        void readTranscript(id, null)
        return {
          ...current,
          [id]: { entries: [], nextCursor: null, unavailable: null, loading: true, outputsOpen: [] },
        }
      })
    }
    // The transcript read rides the state above; the effect answers the link.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessions, expandedSession])

  // A running row is live: every store change re-reads its transcript from the
  // start, so the thinking arrives as the harness records it. An ended row
  // freezes at what its expansion read.
  useEffect(() => {
    if (sessions === null) return
    for (const record of sessions) {
      if (record.status === 'running' && expanded.includes(record.id)) {
        void readTranscript(record.id, null)
      }
    }
    // The reads follow the rows the section drew; the revision only re-times them.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision])

  function toggle(id: string): void {
    const closing = expanded.includes(id)
    const next = closing ? expanded.filter((open) => open !== id) : [...expanded, id]
    setExpanded(next)
    window.location.hash = sheetSessionHref(
      window.location.hash,
      ticketId,
      closing ? null : id,
    )
    if (!closing && transcripts[id] === undefined) {
      void readTranscript(id, null)
    }
  }

  return (
    <section className="sheet__sessions" aria-label="Sessions">
      <h2 className="sheet__section-title">Sessions: the passes that carried this ticket</h2>
      {linkNotice === null ? null : (
        <p className="sheet__session-notice" role="status">
          {linkNotice}
        </p>
      )}
      {readError !== null ? (
        <p className="sheet__refusal" role="alert">
          The sessions could not be read. {readError}
        </p>
      ) : sessions === null ? (
        <p className="sheet__reading">Reading the sessions…</p>
      ) : sessions.length === 0 ? (
        <p className="sheet__empty">No agent sessions are linked to this ticket yet.</p>
      ) : (
        <ul className="sheet__session-rows">
          {sessions.map((record) => {
            const open = expanded.includes(record.id)
            const row = transcripts[record.id]
            const usage = usageLine(record)
            const started = stampLabel(record.startedAt)
            const ended = stampLabel(record.endedAt)
            return (
              <li key={record.id} className="sheet__session-row">
                <button
                  type="button"
                  className="sheet__session-toggle"
                  aria-expanded={open}
                  onClick={() => toggle(record.id)}
                >
                  <span className="sheet__session-header">{headerLabel(record)}</span>
                  <span className="sheet__session-brief">{record.brief}</span>
                  {record.status === 'running' ? (
                    <span className="sheet__session-live">live</span>
                  ) : null}
                </button>
                {record.summary === null || record.summary.trim() === '' ? null : (
                  <p className="sheet__session-summary">{record.summary}</p>
                )}
                <p className="sheet__session-meta">
                  {[usage, started === null ? null : `started ${started}`, ended === null ? null : `ended ${ended}`]
                    .filter((part) => part !== null)
                    .join(' · ') || 'No usage reported.'}
                </p>
                {open ? (
                  <div className="sheet__session-transcript">
                    {row === undefined || row.loading ? (
                      <p className="sheet__reading">Reading the transcript…</p>
                    ) : row.unavailable !== null ? (
                      <p className="sheet__session-notice" role="status">
                        {row.unavailable}
                      </p>
                    ) : row.entries.length === 0 ? (
                      <p className="sheet__empty">No transcript yet.</p>
                    ) : (
                      <>
                        <ol className="sheet__session-entries">
                          {row.entries.map((entry) => (
                            <li key={entry.id} className="sheet__session-entry">
                              <header className="sheet__session-entry-head">
                                <span className="sheet__session-role">{entry.role}</span>
                                {entry.agent === null ? null : (
                                  <span className="sheet__session-agent">{entry.agent}</span>
                                )}
                                {entry.model === null ? null : (
                                  <span className="sheet__session-model">{entry.model}</span>
                                )}
                              </header>
                              {entry.blocks.map((block, index) => (
                                <TranscriptBlockView
                                  key={`${entry.id}-${index}`}
                                  block={block}
                                  outputKey={`${entry.id}-${index}-output`}
                                />
                              ))}
                            </li>
                          ))}
                        </ol>
                        {row.nextCursor === null ? null : (
                          <button
                            type="button"
                            className="sheet__button"
                            onClick={() => void readTranscript(record.id, row.nextCursor)}
                          >
                            Load more
                          </button>
                        )}
                      </>
                    )}
                  </div>
                ) : null}
              </li>
            )
          })}
        </ul>
      )}
    </section>
  )
}
