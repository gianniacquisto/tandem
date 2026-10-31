import type { TicketListReply } from '../wire/TicketListReply'
import type { UsageReportView } from '../wire/UsageReportView'

/**
 * What the shell knows about the store: the last listing's revision, its size,
 * the usage report the Usage face draws, and whether the last read landed. No
 * face computes a number: the snapshot is the only state, the revision is the
 * one the listing carries, and the report is the derivation's (`usage.report`),
 * which carries no revision of its own.
 */
export type SnapshotState = {
  status: 'connecting' | 'ready' | 'failed'
  revision: number | null
  tickets: number
  usage: UsageReportView | null
  error: string | null
}

export type SnapshotAction =
  | { type: 'loaded'; reply: TicketListReply }
  | { type: 'usage'; report: UsageReportView }
  | { type: 'failed'; message: string }

export const initialSnapshot: SnapshotState = {
  status: 'connecting',
  revision: null,
  tickets: 0,
  usage: null,
  error: null,
}

export function snapshotReducer(state: SnapshotState, action: SnapshotAction): SnapshotState {
  if (action.type === 'loaded') {
    return {
      status: 'ready',
      revision: action.reply.revision,
      tickets: action.reply.tickets.length,
      usage: state.usage,
      error: null,
    }
  }
  if (action.type === 'usage') {
    return { ...state, usage: action.report }
  }

  return { ...state, status: 'failed', error: action.message }
}
