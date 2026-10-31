import type { SnapshotState } from './snapshot'

/**
 * Whether a write may leave the face, and why not when it may not.
 *
 * The listing's revision is authoritative (PRT-11): a change event is a hint
 * that the store moved, and a write aimed at a revision the face no longer
 * shows would be a silent overwrite on top of somebody else's write. The guard
 * refuses it and says so instead; the shell re-reads and the human acts on what
 * is actually there.
 */
export function writeBlocked(state: SnapshotState, expected: number | null): string | null {
  if (state.revision === null) {
    return 'The board has not been read yet. Nothing was written.'
  }
  if (state.status === 'failed') {
    return 'The board is not readable right now. Nothing was written.'
  }
  if (state.stale) {
    return 'The board changed while you were acting. Nothing was written.'
  }
  if (expected !== null && state.revision !== expected) {
    return `The board moved from revision ${expected} to revision ${state.revision} while you were acting. Nothing was written.`
  }
  return null
}
