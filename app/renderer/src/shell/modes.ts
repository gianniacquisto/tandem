import type { SaveOutcome } from './note'

/**
 * The bikeshed editor's two modes and the switch between them.
 *
 * Write is the block surface (UI-18); Source is the same note as its exact
 * markdown in a textarea. Preview is retired, so the stored `preview` and
 * `edit` values read as Write rather than migrating storage: a relaunch on an
 * old profile lands on the block surface, which is the one view of the note.
 *
 * The mode is one app preference, not a per-note flag, and it is persisted on
 * an explicit switch, never on open. An empty note always opens Write so a
 * creation is ready to type regardless of the stored choice.
 *
 * No React and no `window`: the rules are pure, so they are tested in Node.
 */
export type Mode = 'write' | 'source'

export const BIKESHED_MODE_KEY = 'tandem.bikeshed.mode'
export const BIKESHED_MODE_DEFAULT: Mode = 'write'

type ModeStorage = {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
}

function modeStorage(storage?: ModeStorage | null): ModeStorage | null {
  if (storage !== undefined) return storage
  const candidate =
    typeof globalThis.localStorage !== 'undefined' ? globalThis.localStorage : null
  if (candidate === null) return null
  if (typeof candidate.getItem !== 'function' || typeof candidate.setItem !== 'function')
    return null
  return candidate as ModeStorage
}

/** The stored choice; a legacy `preview` or `edit`, and nonsense, read as Write. */
export function loadMode(storage?: ModeStorage | null): Mode {
  const store = modeStorage(storage)
  if (store === null) return BIKESHED_MODE_DEFAULT
  const raw = store.getItem(BIKESHED_MODE_KEY)
  return raw === 'source' ? 'source' : BIKESHED_MODE_DEFAULT
}

/** Persist a switch, ignoring nonsense so it cannot stick. */
export function saveMode(mode: Mode, storage?: ModeStorage | null): void {
  const store = modeStorage(storage)
  if (store === null) return
  if (mode !== 'write' && mode !== 'source') return
  store.setItem(BIKESHED_MODE_KEY, mode)
}

/**
 * The mode a note opens in: an empty or whitespace-only note is always Write,
 * so a new note is ready to type; otherwise the stored choice wins.
 */
export function openMode(stored: unknown, text: string): Mode {
  if (text.trim() === '') return 'write'
  return stored === 'source' ? 'source' : BIKESHED_MODE_DEFAULT
}

/** The three faces the switch reads: what the buffer, the save and the banner say. */
export type SwitchState = {
  dirty: boolean
  saving: boolean
  conflict: boolean
}

export type SwitchIntent = 'blocked' | 'wait' | 'flush' | 'switch'

/**
 * What a switch to the other surface does first.
 *
 * The conflict banner freezes the face, so a switch is inert there. A save in
 * flight is awaited, never doubled. A dirty buffer is flushed through the one
 * save path before the surface changes; a clean one switches without a write.
 */
export function switchIntent(state: SwitchState): SwitchIntent {
  if (state.conflict) return 'blocked'
  if (state.saving) return 'wait'
  if (state.dirty) return 'flush'
  return 'switch'
}

/** After a flush: only a save that landed allows the surface to change. */
export function switchAfterFlush(outcome: SaveOutcome['type']): 'switch' | 'stay' {
  return outcome === 'saved' ? 'switch' : 'stay'
}
