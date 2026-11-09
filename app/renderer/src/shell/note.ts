import type { Method, MethodCall } from '../../../src/shared/bridge'
import type { NoteContentView } from '../wire/NoteContentView'
import { parseDocument, serialiseDocument, type Document } from './blocks'

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
  /**
   * The parsed document behind the text. A full parse per posted change, by
   * choice: notes are small and a second, incrementally updated block list
   * would be a second truth.
   */
  document: Document
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

export type NoteCall = (call: Method) => Promise<MethodCall>

/** The buffer an open note starts from: clean, at the hash the read reported. */
export function openBuffer(note: { path: string; text: string; hash: string }): NoteBuffer {
  return {
    path: note.path,
    text: note.text,
    baseline: note.hash,
    dirty: false,
    document: parseDocument(note.text),
  }
}

/** The buffer after an external write arrives: adopt it when clean, conflict when dirty. */
export function arrival(buffer: NoteBuffer, onDisk: Theirs): Arrival {
  if (onDisk.hash === buffer.baseline) return { type: 'unchanged' }
  if (!buffer.dirty) {
    return {
      type: 'adopted',
      buffer: {
        path: buffer.path,
        text: onDisk.text,
        baseline: onDisk.hash,
        dirty: false,
        document: parseDocument(onDisk.text),
      },
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
  return {
    path: buffer.path,
    text: theirs.text,
    baseline: theirs.hash,
    dirty: false,
    document: parseDocument(theirs.text),
  }
}

/**
 * Whether a save's conflict still names the open buffer: the save read the disk
 * against the baseline it carried, so a buffer that has since adopted that disk
 * (Take theirs moved the baseline) must not re-raise the same conflict when the
 * stale save lands after the choice.
 */
export function conflictStillApplies(
  current: NoteBuffer | null,
  saved: NoteBuffer,
): boolean {
  return (
    current !== null && current.path === saved.path && current.baseline === saved.baseline
  )
}

/**
 * The editor's dirty message as data: new typing dirties the buffer, an echo of
 * the text it already holds changes nothing. A same-text post (the frame
 * re-announcing the document it was just handed) must not re-dirty a clean
 * buffer after Take theirs, or the next autosave re-reads the adopted disk.
 */
export function applyDirty(buffer: NoteBuffer | null, text: string): NoteBuffer | null {
  if (buffer === null || buffer.text === text) return buffer
  return { ...buffer, text, dirty: true, document: parseDocument(text) }
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
 * Where a creation lands: the open note's folder, else the vault root.
 *
 * The tree has no selection, so beside-the-open-note needs no new state and
 * matches where the human already is. The root is the empty folder, the same
 * spelling the move's `--into` uses.
 */
export function defaultCreateFolder(openPath: string | null): string {
  if (openPath === null || openPath === '') return ''
  const slash = openPath.lastIndexOf('/')
  return slash === -1 ? '' : openPath.slice(0, slash)
}

/**
 * A prompted note name under its folder, with `.md` added once.
 *
 * Trimming is the face's only normalization: hygiene stays the core's
 * (`BikeshedPath`, one truth), and a refusal surfaces in the banner.
 */
export function resolveNotePath(folder: string, name: string): string {
  const trimmed = name.trim()
  const withExt = trimmed.toLowerCase().endsWith('.md') ? trimmed : `${trimmed}.md`
  return folder === '' ? withExt : `${folder}/${withExt}`
}

/**
 * A prompted folder name under its folder, with edge slashes trimmed.
 *
 * Trimming is the face's only normalization, as with notes: the core refuses
 * what cannot be a path.
 */
export function resolveFolderPath(folder: string, name: string): string {
  const trimmed = name.trim().replace(/^\/+|\/+$/g, '')
  return folder === '' ? trimmed : `${folder}/${trimmed}`
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

  // The bytes the save writes come from the model, never from a silent string
  // walk: a block that cannot render fails the save into the banner instead.
  let text: string
  try {
    text = serialiseDocument(buffer.document)
  } catch (error) {
    const message = error instanceof Error ? error.message : 'The note could not be serialised.'
    return { type: 'error', message }
  }

  const disk = await call({ method: 'note.read', params: { path: buffer.path } })
  if ('error' in disk) return { type: 'error', message: disk.error.message }

  const content = noteContentOf(disk.reply)
  if (content === null) return { type: 'error', message: 'The note could not be read.' }
  if (content.hash !== buffer.baseline) {
    return { type: 'conflict', theirs: { text: content.text, hash: content.hash } }
  }

  const written = await call({
    method: 'note.write',
    params: { path: buffer.path, text },
  })
  if ('error' in written) return { type: 'error', message: written.error.message }

  const hash = hashOf(written.reply)
  if (hash === null) return { type: 'error', message: 'The note could not be written.' }
  return { type: 'saved', hash }
}

/**
 * A dirty buffer whose last save did not land.
 *
 * A tab switch unmounts the face; the pending autosave dies with it. The
 * unmount flush writes when it can, and stashes the buffer on a conflict or an
 * error, so the next visit resumes it. The Swift model outlives the tab; this
 * map is the port's same promise, without a second model.
 */
const drafts = new Map<string, NoteBuffer>()

/** Keep a dirty buffer for the next visit. A clean one has nothing to lose. */
export function stashDraft(buffer: NoteBuffer): void {
  if (buffer.dirty) drafts.set(buffer.path, buffer)
}

/** The oldest kept draft, removed from the map; null when nothing is waiting. */
export function takeDraft(): NoteBuffer | null {
  const first = drafts.values().next()
  if (first.done === true) return null
  drafts.delete(first.value.path)
  return first.value
}
