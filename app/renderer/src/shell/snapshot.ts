import type { TicketListReply } from '../wire/TicketListReply'
import type { TicketSummary } from '../wire/TicketSummary'
import type { UsageReportView } from '../wire/UsageReportView'

/**
 * What the shell knows about the store: the last listing's revision, its rows,
 * the lanes they are grouped into, the usage report the Usage face draws, and
 * whether the last read landed.
 *
 * One listing is the whole store (`filter: all, archived: true`); the face
 * narrows it locally, so the stored order the drag arithmetic needs is always
 * the same array the rows came from. The revision is the listing's own; a change
 * event only marks the snapshot stale until the re-read lands.
 */
export type SnapshotState = {
  status: 'connecting' | 'ready' | 'failed'
  revision: number | null
  tickets: TicketSummary[]
  lanes: string[]
  usage: UsageReportView | null
  error: string | null
  /** A conflict or refusal the human has not dismissed yet. */
  notice: string | null
  /** An event named a store this listing has not caught up to. */
  stale: boolean
}

export type SnapshotAction =
  | { type: 'loaded'; reply: TicketListReply; lanes: string[] }
  | { type: 'usage'; report: UsageReportView }
  | { type: 'failed'; message: string }
  | { type: 'stale' }
  | { type: 'notice'; message: string }
  | { type: 'dismiss' }

export const initialSnapshot: SnapshotState = {
  status: 'connecting',
  revision: null,
  tickets: [],
  lanes: [],
  usage: null,
  error: null,
  notice: null,
  stale: false,
}

export function snapshotReducer(state: SnapshotState, action: SnapshotAction): SnapshotState {
  if (action.type === 'loaded') {
    return {
      ...state,
      status: 'ready',
      revision: action.reply.revision,
      tickets: action.reply.tickets,
      lanes: action.lanes,
      error: null,
      stale: false,
    }
  }
  if (action.type === 'usage') {
    return { ...state, usage: action.report }
  }
  if (action.type === 'stale') {
    return { ...state, stale: true }
  }
  if (action.type === 'notice') {
    return { ...state, notice: action.message }
  }
  if (action.type === 'dismiss') {
    return { ...state, notice: null }
  }

  return { ...state, status: 'failed', stale: false, error: action.message }
}
