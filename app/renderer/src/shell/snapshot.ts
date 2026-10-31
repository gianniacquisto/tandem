import type { TicketListReply } from '../wire/TicketListReply'

/**
 * What the shell knows about the store: the last listing's revision, its size,
 * and whether the last read landed. No face computes a number: the snapshot is
 * the only state, and the revision is the one the listing carries.
 */
export type SnapshotState = {
  status: 'connecting' | 'ready' | 'failed'
  revision: number | null
  tickets: number
  error: string | null
}

export type SnapshotAction =
  | { type: 'loaded'; reply: TicketListReply }
  | { type: 'failed'; message: string }

export const initialSnapshot: SnapshotState = {
  status: 'connecting',
  revision: null,
  tickets: 0,
  error: null,
}

export function snapshotReducer(state: SnapshotState, action: SnapshotAction): SnapshotState {
  if (action.type === 'loaded') {
    return {
      status: 'ready',
      revision: action.reply.revision,
      tickets: action.reply.tickets.length,
      error: null,
    }
  }

  return { ...state, status: 'failed', error: action.message }
}
