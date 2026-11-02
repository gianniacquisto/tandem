import { useEffect, useRef, useState } from 'react'

import type { Method } from '../../../src/shared/bridge'
import { STATUSES, assigneeChoices } from '../shell/board'
import { passButtons, type PassKind } from '../shell/pass'
import {
  EMPTY_COMMENT_REFUSAL,
  answerBlock,
  appendPlan,
  canSave,
  commentAuthor,
  commentParams,
  commentTime,
  holdOpen,
  laneChoices,
  laneCreateParams,
  laneRenameParams,
  sheetChanges,
  sheetOf,
  shortcutHint,
  ticketHandle,
} from '../shell/sheet'
import type { Write } from '../shell/write'
import type { CommentView } from '../wire/CommentView'
import type { MarkdownBlockView } from '../wire/MarkdownBlockView'
import type { Reply } from '../wire/Reply'
import type { TicketDetail } from '../wire/TicketDetail'
import { MarkdownDocument } from './MarkdownDocument'

export type TicketSheetProps = {
  /** The sheet's subject: the one selection every entry point sets. */
  ticketId: string
  /** The board's declared lanes, in render order. */
  lanes: string[]
  /** The names the assignee picker offers, off-roster names aside. */
  roster: string[]
  /** The revision the face last read; the shell's write guard pins to it. */
  revision: number | null
  /** A write through the shell's one guarded door. */
  onWrite: Write
  /** Esc or the close control: the face that opened the sheet is untouched. */
  onDismiss: () => void
  /** A dependency row re-targets the same sheet at another ticket. */
  onOpenTicket: (id: string) => void
}

function commentsOf(reply: Reply): CommentView[] | null {
  return 'comments' in reply ? reply.comments : null
}

function blocksOf(reply: Reply): MarkdownBlockView[] | null {
  return 'blocks' in reply ? reply.blocks : null
}

const CLAIM_CHANGED = 'The claim landed, but the comment did not.'

/**
 * The ticket sheet: one dialog over whichever face opened it, drawn once for
 * the list row, the board card and the note's ticket link (`TicketSheet` in the
 * Swift app).
 *
 * It reads its own ticket and its own log, because the listing the shell holds
 * carries neither a body nor comments; the board, the lanes and the revision
 * still arrive as props, so a write is the shell's guarded one. Every edit is a
 * message the CLI also sends (`ticket.set`, `ticket.move`, `ticket.comment.add`,
 * `lane.add`, `lane.rename`), the body is parsed by the core (`markdown.blocks`)
 * and appended to, never rewritten, and the log is read-only: the sheet's half
 * of it is the composer, which only appends.
 */
export function TicketSheet({
  ticketId,
  lanes,
  roster,
  revision,
  onWrite,
  onDismiss,
  onOpenTicket,
}: TicketSheetProps) {
  const [ticket, setTicket] = useState<TicketDetail | null>(null)
  // The revision the ticket above was read at: the body's pin, not the
  // listing's (`SheetRead`).
  const [sheetRevision, setSheetRevision] = useState<number | null>(null)
  // A refusal re-reads the sheet; the token makes that a state change.
  const [readToken, setReadToken] = useState(0)
  const [comments, setComments] = useState<CommentView[]>([])
  const [commentBlocks, setCommentBlocks] = useState<MarkdownBlockView[][]>([])
  const [descriptionBlocks, setDescriptionBlocks] = useState<MarkdownBlockView[]>([])
  const [answerBlocks, setAnswerBlocks] = useState<MarkdownBlockView[]>([])
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  // The reads that failed, told apart from an empty log or an absent body: a
  // failed read is not "nothing here".
  const [logError, setLogError] = useState<string | null>(null)
  const [markdownError, setMarkdownError] = useState<string | null>(null)

  const [title, setTitle] = useState('')
  const [status, setStatus] = useState('backlog')
  const [assignee, setAssignee] = useState<string | null>(null)
  const [previewing, setPreviewing] = useState(true)
  const [addition, setAddition] = useState('')
  const [descriptionError, setDescriptionError] = useState<string | null>(null)
  const [commentDraft, setCommentDraft] = useState('')
  const [commentError, setCommentError] = useState<string | null>(null)

  const [creating, setCreating] = useState(false)
  const [laneName, setLaneName] = useState('')
  const [laneKey, setLaneKey] = useState('')
  const [asking, setAsking] = useState<{ name: string; abbr: string | null } | null>(null)
  const [renaming, setRenaming] = useState<string | null>(null)
  const [renameTo, setRenameTo] = useState('')
  const [renameKey, setRenameKey] = useState('')

  const draftFor = useRef<string | null>(null)
  const dialog = useRef<HTMLDivElement>(null)

  // The ticket and its log, re-read when the sheet opens, on every store
  // change, and after a refusal: a refusal means the body this sheet holds may
  // no longer be the body the store has, and the next write pins to that read.
  useEffect(() => {
    let live = true
    async function load(): Promise<void> {
      const [shown, log] = await Promise.all([
        window.tandem.call({ method: 'ticket.show', params: { id: ticketId } }),
        window.tandem.call({ method: 'ticket.comment.list', params: { id: ticketId } }),
      ])
      if (!live) return
      if ('error' in shown) {
        setError(shown.error.message)
        return
      }
      const read = sheetOf(shown.reply)
      if (read === null) {
        setError('The ticket could not be read.')
        return
      }
      const found = read.ticket
      const logFailure = 'error' in log ? log.error.message : null
      const logRows = 'error' in log ? [] : (commentsOf(log.reply) ?? [])
      const decision = answerBlock(found.description)
      const [body, answerBody, ...logBodies] = await Promise.all([
        window.tandem.call({
          method: 'markdown.blocks',
          params: { text: found.description },
        }),
        window.tandem.call({ method: 'markdown.blocks', params: { text: decision } }),
        ...logRows.map((comment) =>
          window.tandem.call({ method: 'markdown.blocks', params: { text: comment.body } }),
        ),
      ])
      if (!live) return
      setError(null)
      setTicket(found)
      // The pin follows every read, not only the opening: a re-read after a
      // refusal must hand the next write the revision the stored body is at.
      setSheetRevision(read.revision)
      setComments(logRows)
      const bodyBlocks = 'error' in body ? [] : (blocksOf(body.reply) ?? [])
      // One failed markdown read is a failure to say out loud, never an empty
      // body: the text is stored, it just could not be drawn.
      let markdownFailure = 'error' in body ? body.error.message : null
      if (markdownFailure === null && 'error' in answerBody) {
        markdownFailure = answerBody.error.message
      }
      for (const reply of logBodies) {
        if (markdownFailure === null && 'error' in reply) markdownFailure = reply.error.message
      }
      setDescriptionBlocks(bodyBlocks)
      setAnswerBlocks('error' in answerBody ? [] : (blocksOf(answerBody.reply) ?? []))
      setCommentBlocks(
        logBodies.map((reply) => ('error' in reply ? [] : (blocksOf(reply.reply) ?? []))),
      )
      setLogError(logFailure)
      setMarkdownError(markdownFailure)
      // A ticket with a body opens for reading it; a terse one opens for
      // writing it (the Swift sheet's rule). Seeded once per opening: a
      // re-read must not overwrite what the human is typing.
      if (draftFor.current !== found.id) {
        draftFor.current = found.id
        dialog.current?.focus()
        setTitle(found.title)
        setStatus(found.status)
        setAssignee(found.assignee)
        setPreviewing(bodyBlocks.length > 0)
        setAddition('')
        setDescriptionError(null)
        setCommentDraft('')
        setCommentError(null)
        setNotice(null)
      }
    }
    void load()
    return () => {
      live = false
    }
  }, [ticketId, revision, readToken])

  useEffect(() => {
    function handle(event: KeyboardEvent): void {
      if (event.key === 'Escape') {
        event.preventDefault()
        onDismiss()
        return
      }
      if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
        event.preventDefault()
        if (event.shiftKey) void postComment()
        else void save()
      }
    }
    window.addEventListener('keydown', handle)
    return () => window.removeEventListener('keydown', handle)
    // The handlers read the sheet's current state through its render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ticket, title, status, assignee, commentDraft, revision, sheetRevision])

  async function write(
    call: Method,
    expected: number | null = revision,
    followUp?: string,
  ): Promise<boolean> {
    const outcome = await onWrite(call, expected, followUp)
    if (!outcome.ok) {
      setNotice(outcome.message)
      // A refusal means the store moved under the sheet, or it would not take
      // the write: read the ticket again, so the next write pins to what is
      // actually stored and the body the human sees is the body being appended
      // to.
      setReadToken((token) => token + 1)
      return false
    }
    return true
  }

  async function save(): Promise<void> {
    if (ticket === null || !canSave({ title })) return
    const changes = sheetChanges(ticket, { title, status, assignee })
    if (changes === null) {
      setNotice('Nothing changed.')
      return
    }
    const written = await write(
      {
        method: 'ticket.set',
        params: { id: ticket.id, title: null, status: null, assignee: null, description: null, ...changes },
      },
      // The draft's fields came from the ticket read; pin them there too, so a
      // concurrent title change is a conflict and not a silent overwrite.
      sheetRevision ?? revision,
    )
    if (written) setNotice(null)
  }

  async function append(): Promise<void> {
    if (ticket === null || sheetRevision === null) return
    // The body comes from the ticket read, so the write pins to that read: a
    // `ticket answer` that landed between the read and this append answers a
    // conflict, and the sheet reads again, instead of erasing the answer.
    const plan = appendPlan({ ticket, revision: sheetRevision }, addition)
    if (!plan.ok) {
      setDescriptionError(plan.message)
      return
    }
    setDescriptionError(null)
    const written = await write(plan.call, plan.expected)
    if (written) setAddition('')
  }

  async function postComment(): Promise<void> {
    if (ticket === null) return
    const params = commentParams(ticket.id, commentDraft)
    if (params === null) {
      setCommentError(EMPTY_COMMENT_REFUSAL)
      return
    }
    setCommentError(null)
    const written = await write({ method: 'ticket.comment.add', params })
    if (written) setCommentDraft('')
  }

  /**
   * One pass request's writes, in the helper's order. A second write is a
   * follow-up: the first one moved the listing, so it is exempt from the
   * revision guard and its refusal says that the claim already landed.
   */
  async function requestPass(kind: PassKind): Promise<void> {
    if (ticket === null) return
    const button = passButtons(
      { id: ticket.id, status: ticket.status, blocked: ticket.blocked, assignee: ticket.assignee },
      comments.map((comment) => comment.body),
    ).find((entry) => entry.kind === kind)
    if (button === undefined || !button.enabled) return
    for (const [at, call] of button.calls.entries()) {
      const written = await write(
        call,
        // The claim edits the ticket the sheet read, so it pins to that read;
        // the comment follows the claim and is exempt from the guard.
        call.method === 'ticket.set' ? (sheetRevision ?? revision) : null,
        at === 0 ? undefined : CLAIM_CHANGED,
      )
      if (!written) return
    }
    setNotice(null)
  }

  async function moveToLane(lane: string | null): Promise<void> {
    if (ticket === null || ticket.lane === lane) return
    // A move appends to the lane, the same instruction a drop makes; the lane
    // is not a `ticket.set` field, and a fabricated `set` would be a UI-only
    // write the CLI cannot make.
    const written = await write({
      method: 'ticket.move',
      params: { id: ticket.id, lane, index: null },
    })
    if (written) setNotice(null)
  }

  function askLane(): void {
    const params = laneCreateParams(laneName, laneKey)
    if (params === null) return
    setAsking(params)
  }

  async function createLane(): Promise<void> {
    if (asking === null) return
    const written = await write({
      method: 'lane.add',
      params: { name: asking.name, abbr: asking.abbr },
    })
    if (written) {
      setAsking(null)
      setCreating(false)
      setLaneName('')
      setLaneKey('')
      setNotice(null)
    }
  }

  async function renameLane(): Promise<void> {
    if (renaming === null) return
    const params = laneRenameParams(renaming, renameTo, renameKey)
    if (params === null) {
      setRenaming(null)
      return
    }
    const written = await write({
      method: 'lane.rename',
      params: { from: params.from, to: params.to, abbr: params.abbr },
    })
    if (written) {
      setRenaming(null)
      setRenameTo('')
      setRenameKey('')
      setNotice(null)
    }
  }

  const handle = ticket === null ? 'Ticket' : ticketHandle(ticket)
  const choices = assigneeChoices(roster, assignee)
  const lanesShown = laneChoices(ticket?.lane ?? null, lanes)
  const answer = ticket === null ? null : answerBlock(ticket.description)

  return (
    <div
      className="sheet-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onDismiss()
      }}
    >
      <div
        ref={dialog}
        className="sheet"
        role="dialog"
        aria-modal="true"
        aria-label={handle}
        tabIndex={-1}
      >
        <header className="sheet__header">
          <span className="sheet__handle">{handle}</span>
          {ticket === null ? null : (
            <span className="sheet__origin">
              {ticket.origin === 'human' ? 'you filed' : `@${ticket.origin} filed`}
            </span>
          )}
          {ticket?.link === null || ticket?.link === undefined ? null : (
            <span className="sheet__link" title={ticket.link}>
              {ticket.link}
            </span>
          )}
          <button type="button" className="sheet__close" aria-label="Close the ticket" onClick={onDismiss}>
            ×
          </button>
        </header>

        {error === null ? null : (
          <p className="sheet__error" role="alert">
            {error}
          </p>
        )}

        {notice === null ? null : (
          <div className="sheet__notice" role="alert">
            <span>{notice}</span>
            <button type="button" className="sheet__dismiss" onClick={() => setNotice(null)}>
              Dismiss
            </button>
          </div>
        )}

        {ticket === null ? (
          error === null ? <p className="sheet__reading">Reading the ticket…</p> : null
        ) : (
          <div className="sheet__body">
            <input
              className="sheet__title"
              value={title}
              aria-label="Title"
              onChange={(event) => setTitle(event.target.value)}
            />

            <div className="sheet__facts">
              <label className="sheet__fact">
                <span className="sheet__fact-label">Status</span>
                <select
                  className="sheet__select"
                  aria-label="Status"
                  value={status}
                  onChange={(event) => setStatus(event.target.value)}
                >
                  {STATUSES.map((entry) => (
                    <option key={entry.value} value={entry.value}>
                      {entry.label}
                    </option>
                  ))}
                </select>
              </label>

              <label className="sheet__fact">
                <span className="sheet__fact-label">Assignee</span>
                <select
                  className="sheet__select"
                  aria-label="Assignee"
                  value={assignee ?? ''}
                  onChange={(event) =>
                    setAssignee(event.target.value === '' ? null : event.target.value)
                  }
                >
                  {choices.map((choice) => (
                    <option key={choice.value ?? ''} value={choice.value ?? ''}>
                      {choice.label}
                    </option>
                  ))}
                </select>
              </label>

              <button
                type="button"
                className="sheet__save"
                disabled={!canSave({ title })}
                onClick={() => void save()}
              >
                Save
              </button>
            </div>

            <section className="sheet__passes" aria-label="Agent passes">
              <h2 className="sheet__section-title">
                Agent passes: a request the next dispatch picks up, never a run
              </h2>
              <ul className="sheet__pass-rows">
                {passButtons(
                  {
                    id: ticket.id,
                    status: ticket.status,
                    blocked: ticket.blocked,
                    assignee: ticket.assignee,
                  },
                  comments.map((comment) => comment.body),
                ).map((button) => (
                  <li key={button.kind} className="sheet__pass-row">
                    <button
                      type="button"
                      className="sheet__button"
                      disabled={!button.enabled}
                      onClick={() => void requestPass(button.kind)}
                    >
                      {button.label}
                    </button>
                    {button.reason === null ? null : (
                      <span className="sheet__pass-reason">{button.reason}</span>
                    )}
                  </li>
                ))}
              </ul>
            </section>

            {holdOpen(ticket) ? (
              <p className="sheet__hold" role="status">
                Waiting on the human: this ticket is on hold and no answer block is written yet.
              </p>
            ) : null}

            <section className="sheet__dependencies" aria-label="Dependencies">
              {ticket.blockedBy.length === 0 && ticket.blocks.length === 0 ? null : (
                <>
                  {ticket.blockedBy.length === 0 ? null : (
                    <>
                      <h2 className="sheet__section-title">Blocked by</h2>
                      <ul className="sheet__edges">
                        {ticket.blockedBy.map((block) => (
                          <li key={block.id}>
                            <button
                              type="button"
                              className="sheet__edge"
                              onClick={() => onOpenTicket(block.id)}
                            >
                              <span className="sheet__edge-ref">{block.ref ?? 'no ref'}</span>
                              <span className="sheet__edge-title">{block.title}</span>
                              <span className="sheet__edge-status">{STATUSES.find((entry) => entry.value === block.status)?.label ?? block.status}</span>
                            </button>
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                  {ticket.blockedReason === null ? null : (
                    <p className="sheet__blocked-reason">{ticket.blockedReason}</p>
                  )}
                  {ticket.blocks.length === 0 ? null : (
                    <>
                      <h2 className="sheet__section-title">Blocks</h2>
                      <ul className="sheet__edges">
                        {ticket.blocks.map((block) => (
                          <li key={block.id}>
                            <button
                              type="button"
                              className="sheet__edge"
                              onClick={() => onOpenTicket(block.id)}
                            >
                              <span className="sheet__edge-ref">{block.ref ?? 'no ref'}</span>
                              <span className="sheet__edge-title">{block.title}</span>
                              <span className="sheet__edge-status">{STATUSES.find((entry) => entry.value === block.status)?.label ?? block.status}</span>
                            </button>
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                </>
              )}
            </section>

            <section className="sheet__description" aria-label="Description">
              <header className="sheet__section-header">
                <h2 className="sheet__section-title">Description: markdown, saved with the ticket</h2>
                <button
                  type="button"
                  className="sheet__toggle"
                  onClick={() => setPreviewing((current) => !current)}
                >
                  {previewing ? 'Edit source' : 'Preview'}
                </button>
              </header>

              {previewing ? (
                <div className="sheet__preview">
                  {markdownError !== null ? (
                    <p className="sheet__refusal" role="alert">
                      The description could not be drawn. {markdownError}
                    </p>
                  ) : descriptionBlocks.length === 0 ? (
                    <p className="sheet__empty">No description yet.</p>
                  ) : (
                    <MarkdownDocument blocks={descriptionBlocks} />
                  )}
                </div>
              ) : (
                <pre className="sheet__source">{ticket.description ?? ''}</pre>
              )}

              <label className="sheet__addition">
                <span className="sheet__fact-label">Add to the description</span>
                <textarea
                  className="sheet__addition-field"
                  aria-label="Add to the description"
                  rows={4}
                  value={addition}
                  placeholder="Appended under the stored body, never over it."
                  onChange={(event) => setAddition(event.target.value)}
                />
              </label>
              {descriptionError === null ? null : (
                <p className="sheet__refusal" role="alert">
                  {descriptionError}
                </p>
              )}
              <div className="sheet__actions">
                <button type="button" className="sheet__button" onClick={() => void append()}>
                  Append
                </button>
              </div>
            </section>

            {answer === null ? null : (
              <section className="sheet__answer" aria-label="Answer">
                <h2 className="sheet__section-title">Answer</h2>
                <MarkdownDocument blocks={answerBlocks} />
              </section>
            )}

            <section className="sheet__lanes" aria-label="Lanes">
              <header className="sheet__section-header">
                <h2 className="sheet__section-title">Lanes</h2>
                {creating ? null : (
                  <button
                    type="button"
                    className="sheet__toggle"
                    onClick={() => setCreating(true)}
                  >
                    New cycle lane
                  </button>
                )}
              </header>
              <ul className="sheet__lane-rows">
                {lanesShown.map((lane) => (
                  <li key={lane.value ?? 'default'} className="sheet__lane-row" data-lane={lane.value ?? ''}>
                    <button
                      type="button"
                      className="sheet__lane"
                      aria-pressed={lane.selected}
                      onClick={() => void moveToLane(lane.value)}
                    >
                      <span className="sheet__lane-marker" aria-hidden="true" />
                      <span className="sheet__lane-name">{lane.label}</span>
                      {lane.selected ? (
                        <span className="sheet__lane-current">this ticket</span>
                      ) : null}
                    </button>
                    {lane.value === null ? null : renaming === lane.value || !lane.renamable ? null : (
                      <button
                        type="button"
                        className="sheet__lane-rename"
                        onClick={() => {
                          if (lane.value === null) return
                          setRenaming(lane.value)
                          setRenameTo(lane.value)
                          setRenameKey('')
                        }}
                      >
                        Rename
                      </button>
                    )}
                    {renaming === null || renaming !== lane.value ? null : (
                      <form
                        className="sheet__lane-form"
                        onSubmit={(event) => {
                          event.preventDefault()
                          void renameLane()
                        }}
                      >
                        <input
                          className="sheet__field"
                          aria-label="Lane name"
                          value={renameTo}
                          onChange={(event) => setRenameTo(event.target.value)}
                        />
                        <input
                          className="sheet__field"
                          aria-label="Lane key"
                          placeholder="Key"
                          value={renameKey}
                          onChange={(event) => setRenameKey(event.target.value)}
                        />
                        <button type="submit" className="sheet__button">
                          Rename
                        </button>
                      </form>
                    )}
                  </li>
                ))}
              </ul>

              {creating ? (
                asking === null ? (
                  <form
                    className="sheet__lane-form"
                    onSubmit={(event) => {
                      event.preventDefault()
                      askLane()
                    }}
                  >
                    <input
                      className="sheet__field"
                      aria-label="New lane name"
                      placeholder="Cycle lane name"
                      value={laneName}
                      onChange={(event) => setLaneName(event.target.value)}
                    />
                    <input
                      className="sheet__field"
                      aria-label="New lane key"
                      placeholder="Key"
                      value={laneKey}
                      onChange={(event) => setLaneKey(event.target.value)}
                    />
                    <button type="submit" className="sheet__button" disabled={laneName.trim() === ''}>
                      Ask
                    </button>
                  </form>
                ) : (
                  <div className="sheet__ask" role="status">
                    <span>
                      Create the cycle lane “{asking.name}”
                      {asking.abbr === null ? ' with a key the core issues' : ` keyed ${asking.abbr}`}?
                    </span>
                    <button type="button" className="sheet__button" onClick={() => void createLane()}>
                      Create lane
                    </button>
                    <button type="button" className="sheet__toggle" onClick={() => setAsking(null)}>
                      Cancel
                    </button>
                  </div>
                )
              ) : null}
            </section>

            <section className="sheet__discussion" aria-label="Comments">
              <h2 className="sheet__section-title">Comments: the ticket's discussion log</h2>
              {logError !== null ? (
                <p className="sheet__refusal" role="alert">
                  The log could not be read. {logError}
                </p>
              ) : comments.length === 0 ? (
                <p className="sheet__empty">Nothing in the log yet.</p>
              ) : (
                <ul className="sheet__comments">
                  {comments.map((comment, index) => (
                    <li key={`${comment.at}-${index}`} className="sheet__comment">
                      <header className="sheet__comment-header">
                        <span className="sheet__comment-author">
                          {commentAuthor(comment.author)}
                        </span>
                        <span className="sheet__comment-time">{commentTime(comment.at)}</span>
                      </header>
                      <MarkdownDocument blocks={commentBlocks[index] ?? []} />
                    </li>
                  ))}
                </ul>
              )}

              <label className="sheet__addition">
                <span className="sheet__fact-label">Add a comment: appended to the log, signed as you</span>
                <textarea
                  className="sheet__addition-field"
                  aria-label="Add a comment"
                  rows={3}
                  value={commentDraft}
                  placeholder="Write a comment…"
                  onChange={(event) => setCommentDraft(event.target.value)}
                />
              </label>
              {commentError === null ? null : (
                <p className="sheet__refusal" role="alert">
                  {commentError}
                </p>
              )}
              <div className="sheet__actions">
                <button type="button" className="sheet__button" onClick={() => void postComment()}>
                  Post
                </button>
              </div>
            </section>

            <footer className="sheet__footer">
              <span className="sheet__hint">{shortcutHint()}</span>
            </footer>
          </div>
        )}
      </div>
    </div>
  )
}
