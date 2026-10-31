import type { BridgeCall, Call } from '../../../src/shared/bridge'
import type { NoteContentView } from '../wire/NoteContentView'

/**
 * The save discipline, as plain data (`BikeshedModel.performSave` in Swift).
 *
 * An editor that loses a keystroke loses a human's note, so the rule is one
 * sentence: a dirty buffer is written only when the disk still holds the hash
 * the buffer last agreed with. A different hash is a conflict, and the conflict
 * carries both bodies; nothing is written until the human chooses. This module
 * holds no React and no `window`, so the rule is tested in Node.
 */
export type NoteBuffer = {
  path: string
  text: string
  /** The on-disk hash the buffer last agreed with. */
  baseline: string
  dirty: boolean
}

export type Theirs = { text: string; hash: string }

export type SaveOutcome =
  | { type: 'saved'; hash: string }
  | { type: 'conflict'; theirs: Theirs }
  | { type: 'error'; message: string }

export type Arrival =
  | { type: 'conflict'; theirs: Theirs }
  | { type: 'adopted'; buffer: NoteBuffer }
  | { type: 'unchanged' }

export type NoteCall = (call: Call) => Promise<BridgeCall>

/** The buffer an open note starts from: clean, at the hash the read reported. */
export function openBuffer(note: { path: string; text: string; hash: string }): NoteBuffer {
  return { path: note.path, text: note.text, baseline: note.hash, dirty: false }
}

/** The buffer after an external write arrives: adopt it when clean, conflict when dirty. */
export function arrival(buffer: NoteBuffer, onDisk: Theirs): Arrival {
  if (onDisk.hash === buffer.baseline) return { type: 'unchanged' }
  if (!buffer.dirty) {
    return {
      type: 'adopted',
      buffer: { path: buffer.path, text: onDisk.text, baseline: onDisk.hash, dirty: false },
    }
  }
  return { type: 'conflict', theirs: onDisk }
}

/**
 * "Keep mine": the baseline moves to the disk this is about to replace, so the
 * save that follows writes the buffer instead of detecting the same conflict
 * again.
 */
export function keepMine(buffer: NoteBuffer, theirs: Theirs): NoteBuffer {
  return { ...buffer, baseline: theirs.hash, dirty: true }
}

/** "Take theirs": the on-disk body replaces the buffer and the edits are dropped. */
export function takeTheirs(buffer: NoteBuffer, theirs: Theirs): NoteBuffer {
  return { path: buffer.path, text: theirs.text, baseline: theirs.hash, dirty: false }
}

/** A note read reply, or null for any other reply. */
export function noteContentOf(reply: unknown): NoteContentView | null {
  if (!isRecord(reply)) return null
  const note = reply['note']
  if (!isRecord(note)) return null
  if (typeof note['hash'] !== 'string' || typeof note['text'] !== 'string') return null
  return note as NoteContentView
}

function hashOf(reply: unknown): string | null {
  if (!isRecord(reply)) return null
  const write = reply['write']
  if (!isRecord(write)) return null
  return typeof write['hash'] === 'string' ? write['hash'] : null
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

/**
 * Save the buffer: read the disk, compare the hash, write only if it agrees.
 *
 * The read-and-write is not atomic, exactly as the Swift saved it: the window
 * between them is the app's own, and the core's write is one locked atomic
 * transaction. The hash check is what turns a lost write into a visible
 * conflict, not a lock.
 */
export async function saveNote(call: NoteCall, buffer: NoteBuffer): Promise<SaveOutcome> {
  if (!buffer.dirty) return { type: 'saved', hash: buffer.baseline }

  const disk = await call({ method: 'note.read', params: { path: buffer.path } })
  if ('error' in disk) return { type: 'error', message: disk.error.message }

  const content = noteContentOf(disk.reply)
  if (content === null) return { type: 'error', message: 'The note could not be read.' }
  if (content.hash !== buffer.baseline) {
    return { type: 'conflict', theirs: { text: content.text, hash: content.hash } }
  }

  const written = await call({
    method: 'note.write',
    params: { path: buffer.path, text: buffer.text },
  })
  if ('error' in written) return { type: 'error', message: written.error.message }

  const hash = hashOf(written.reply)
  if (hash === null) return { type: 'error', message: 'The note could not be written.' }
  return { type: 'saved', hash }
}
