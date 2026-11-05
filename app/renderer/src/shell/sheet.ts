import type { Method } from '../../../src/shared/bridge'
import type { CommentView } from '../wire/CommentView'
import type { MarkdownBlockView } from '../wire/MarkdownBlockView'
import type { Reply } from '../wire/Reply'
import type { TicketDetail } from '../wire/TicketDetail'

import { UNCATEGORIZED, assigneeLabel } from './board'
import { modifierLabel } from './keys'

/**
 * The ticket sheet's rules, as data: what a save sends, what an append does to
 * a description, which lane is the ticket's own, and the refusals the sheet
 * must show in the domain's words.
 *
 * The sheet itself is a thin drawing over these: the arithmetic and the wording
 * live here, in Node, so a rule the domain owns cannot be re-invented in JSX. A
 * description is written once and appended to (ADR 0024), so the sheet's edit is
 * an addition over the stored body, sent as one whole body: the same instruction
 * the CLI's `ticket set --description -` makes.
 */

/** The app is the human's window: a comment typed here is signed as the human. */
export const HUMAN_AUTHOR = 'human'

/**
 * The sheet's own keys, printed in the platform's modifier: the handler reads
 * meta or ctrl as one key, so the hint names the one this keyboard has.
 */
export function shortcutHint(): string {
  const mod = modifierLabel()
  return `${mod}↩ saves · ${mod}⇧↩ posts · Esc closes`
}

/**
 * `DomainError::EmptyDescriptionBody`, verbatim: the CLI's refusal, shown by the
 * sheet rather than a second wording of it. `sheet.test.ts` reads the core's
 * source and fails when this copy drifts.
 */
export const EMPTY_DESCRIPTION_REFUSAL =
  'Description body is empty. Provide a non-empty body on stdin, or pass --description "" to clear the description.'

/** `DomainError::EmptyCommentBody`, verbatim. */
export const EMPTY_COMMENT_REFUSAL =
  'Comment body is empty. Provide a non-empty body and try again.'

export type AppendOutcome = { ok: true; body: string } | { ok: false; message: string }

/**
 * One `ticket.show` answer: the ticket, and the revision that body is a
 * snapshot of. The revision is the write's pin: the body a description append
 * re-sends came from this read, and a write pinned anywhere else could put an
 * older body over an answer that landed in between.
 */
export type SheetRead = { ticket: TicketDetail; revision: number }

/** The ticket and its revision out of a `ticket.show` reply, or null. */
export function sheetOf(reply: Reply): SheetRead | null {
  // A reply that is not an object names no ticket: the sheet reads it as a
  // refusal, never as a throw (TAN-312).
  if (reply === null || reply === undefined || typeof reply !== 'object') return null
  if (!('ticket' in reply) || !('revision' in reply)) return null
  if (reply.ticket === null || typeof reply.ticket !== 'object') return null
  if (typeof reply.revision !== 'number') return null
  return { ticket: reply.ticket, revision: reply.revision }
}

/** The log rows out of a `ticket.comment.list` reply, or null when it names none. */
export function commentsOf(reply: Reply): CommentView[] | null {
  if (reply === null || reply === undefined || typeof reply !== 'object') return null
  if (!('comments' in reply)) return null
  return Array.isArray(reply.comments) ? reply.comments : null
}

/** The blocks out of a `markdown.blocks` reply, or null when it names none. */
export function blocksOf(reply: Reply): MarkdownBlockView[] | null {
  if (reply === null || reply === undefined || typeof reply !== 'object') return null
  if (!('blocks' in reply)) return null
  return Array.isArray(reply.blocks) ? reply.blocks : null
}

function soundEdge(edge: TicketDetail['blockedBy'][number]): boolean {
  return edge !== null && typeof edge === 'object' && typeof edge.id === 'string'
}

/**
 * The ticket the sheet draws: edges that are not arrays draw as none, edges
 * without an id draw as absent, and a description that is not text draws as
 * untold. The store promises the shapes, but one drifting row must still open
 * instead of blanking the app (TAN-312).
 */
export function sheetTicket(ticket: TicketDetail): TicketDetail {
  return {
    ...ticket,
    blockedBy: Array.isArray(ticket.blockedBy) ? ticket.blockedBy.filter(soundEdge) : [],
    blocks: Array.isArray(ticket.blocks) ? ticket.blocks.filter(soundEdge) : [],
    description: typeof ticket.description === 'string' ? ticket.description : null,
  }
}

/**
 * The log rows the sheet draws: rows that are not objects fall out, and fields
 * that are not text draw as empty (the author reads `anonymous`, the stamp
 * reads blank, the body reads nothing). One drifting comment must not take the
 * sheet down with it (TAN-312).
 */
export function sheetComments(comments: CommentView[]): CommentView[] {
  return comments
    .filter((comment) => comment !== null && typeof comment === 'object')
    .map((comment) => ({
      at: typeof comment.at === 'string' ? comment.at : '',
      author: typeof comment.author === 'string' ? comment.author : '',
      body: typeof comment.body === 'string' ? comment.body : '',
    }))
}

/**
 * The body an appended edit writes: the stored text, one blank line, then the
 * addition. `stored` alone is what the append is for, so an addition that
 * carries nothing is the domain's refusal and nothing is written: a pipe of
 * whitespace must never erase a brief (ADR 0024).
 */
export function appendDescription(
  stored: string | null,
  addition: string,
): AppendOutcome {
  const added = addition.trim()
  if (added === '') return { ok: false, message: EMPTY_DESCRIPTION_REFUSAL }
  const existing = stored?.trim() ?? ''
  return { ok: true, body: existing === '' ? added : `${existing}\n\n${added}` }
}

/**
 * Whether the sheet's Details disclosure starts open: it does not. The section
 * holds Status, Assignee, passes, Dependencies, Lanes and Sessions behind one
 * collapsed disclosure, and the disclosure remembers nothing across openings:
 * every opening draws it closed, with no persistence.
 */
export function detailsInitiallyOpen(): boolean {
  return false
}

export type SheetDraft = {
  title: string
  status: string
  assignee: string | null
}

/**
 * The description append as one guarded write: the whole body, and the revision
 * the body was read at. The sheet's body is not the listing's, so the pin is the
 * read's own and never the listing's: a concurrent write that landed between the
 * two would otherwise be overwritten by the older body.
 */
export type AppendPlan =
  | { ok: true; call: Method; expected: number }
  | { ok: false; message: string }

export function appendPlan(read: SheetRead, addition: string): AppendPlan {
  const outcome = appendDescription(read.ticket.description, addition)
  if (!outcome.ok) return outcome
  return {
    ok: true,
    expected: read.revision,
    call: {
      method: 'ticket.set',
      params: {
        id: read.ticket.id,
        title: null,
        status: null,
        assignee: null,
        description: outcome.body,
      },
    },
  }
}

/** The fields moved since the sheet opened, or null when the save is free. */
export type SheetChanges = {
  title?: string
  status?: string
  assignee?: string
} | null

/**
 * What `ticket.set` is told: only the fields that moved. The assignee's clear is
 * the empty string (the core's spelling for Unassigned), and an absent field is
 * unchanged, so a sheet that opened, read, and saved writes nothing.
 */
export function sheetChanges(
  stored: Pick<TicketDetail, 'title' | 'status' | 'assignee'>,
  draft: SheetDraft,
): SheetChanges {
  const changes: NonNullable<SheetChanges> = {}
  const title = draft.title.trim()
  if (title !== stored.title) changes.title = title
  if (draft.status !== stored.status) changes.status = draft.status
  if (draft.assignee !== stored.assignee) changes.assignee = draft.assignee ?? ''
  return Object.keys(changes).length === 0 ? null : changes
}

/** A save needs a title, exactly as the domain's `EmptyTitle` would. */
export function canSave(draft: Pick<SheetDraft, 'title'>): boolean {
  return draft.title.trim() !== ''
}

/** The handle a human writes in chat: the ref, or the number when it has none. */
export function ticketHandle(ticket: Pick<TicketDetail, 'ref' | 'number'>): string {
  if (ticket.ref !== null && ticket.ref !== undefined) return ticket.ref
  if (ticket.number !== null && ticket.number !== undefined) return `#${ticket.number}`
  return '#?'
}

export type LaneChoice = {
  value: string | null
  label: string
  selected: boolean
  /**
   * Whether the store declares the lane. A name only a ticket carries has no
   * row to rename: `lane.rename` on it answers `UnknownLane`, so the sheet
   * hides the control rather than offering the refusal.
   */
  renamable: boolean
}

/**
 * The lane surface's list: the default area, the declared lanes, then any name
 * only a ticket carries (a hand edit cannot hide it), with the marker on the
 * lane the ticket is in. `null` is the default area, which is a real lane.
 */
export function laneChoices(
  current: string | null | undefined,
  lanes: readonly string[] | null | undefined,
): LaneChoice[] {
  const known: Array<string | null> = [null]
  for (const name of lanes ?? []) if (!known.includes(name)) known.push(name)
  const at = current ?? null
  if (at !== null && !known.includes(at)) known.push(at)
  return known.map((value) => ({
    value,
    label: value ?? UNCATEGORIZED,
    selected: value === at,
    renamable: value !== null && (lanes ?? []).includes(value),
  }))
}

/**
 * What the "create a cycle lane" ask would send. A blank name makes no ask: the
 * form's button is disabled and nothing is proposed. The key is optional, and
 * the core issues one when it is absent (`LaneKey::generate`).
 */
export function laneCreateParams(
  name: string,
  key: string,
): { name: string; abbr: string | null; project: string | null } | null {
  const trimmed = name.trim()
  if (trimmed === '') return null
  const abbr = key.trim().toUpperCase()
  return { name: trimmed, abbr: abbr === '' ? null : abbr, project: null }
}

/**
 * What a rename would send, or null when it would change nothing: the same name
 * and no new key is not a write. The domain owns the refusals (a duplicate, a
 * key another lane holds, a frozen key), and the sheet shows them as they come.
 */
export function laneRenameParams(
  from: string,
  to: string,
  key: string,
): { from: string; to: string; abbr: string | null; project: string | null } | null {
  const trimmed = to.trim()
  if (trimmed === '') return null
  const abbr = key.trim().toUpperCase()
  if (trimmed === from && abbr === '') return null
  return { from, to: trimmed, abbr: abbr === '' ? null : abbr, project: null }
}

/** The human's comment, as `ticket.comment.add` takes it, or null when empty. */
export function commentParams(
  id: string,
  body: string,
): { id: string; author: string; body: string } | null {
  const text = body.trim()
  if (text === '') return null
  return { id, author: HUMAN_AUTHOR, body: text }
}

/** An unnamed author is the empty string, and the sheet says so. */
export function commentAuthor(author: string | null | undefined): string {
  return typeof author === 'string' && author.trim() !== '' ? author : 'anonymous'
}

/**
 * A comment's stamp as the sheet writes it: the ISO instant's own date and
 * minute. The store's stamps are UTC, and a sheet that shows no zone would make
 * a reader guess, so it shows the stored spelling and nothing invented.
 */
export function commentTime(at: string | null | undefined): string {
  return typeof at === 'string' ? at.replace('T', ' ').slice(0, 16) : ''
}
