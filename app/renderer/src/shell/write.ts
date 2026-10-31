import type { Call } from '../../../src/shared/bridge'
import type { SnapshotState } from './snapshot'

/**
 * One write's answer. `ok` is whether the store took it; a refusal carries the
 * core's own words, so the face that made the write can answer the action where
 * it was taken instead of raising the shell's banner over somebody else's face.
 */
export type WriteOutcome = { ok: true } | { ok: false; message: string }

/** The shell's one write door: guarded by the listing's revision. */
export type Write = (call: Call, expected: number | null) => Promise<WriteOutcome>

/**
 * Who made a write. A face has no place of its own to answer a refusal, so it
 * goes on the shell's banner; the sheet draws the same words beside the field
 * that raised them, and a banner behind the modal would be a second copy that
 * outlives Esc.
 */
export type WriteCaller = 'face' | 'sheet'

/** The banner a refusal raises, or null when the caller carries the words itself. */
export function refusalBanner(caller: WriteCaller, message: string): string | null {
  return caller === 'sheet' ? null : message
}

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
