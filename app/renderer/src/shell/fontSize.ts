/**
 * The persisted note font size (the toolbar's A-/A+ control, UI-27).
 *
 * The value is the reading size both Write and Source apply through
 * `--tandem-editor-font-size`: a single preference, 16px by default and
 * clamped to 11-20px so a stray write cannot make a note unreadable. It lived
 * in `shell/editor.ts` beside the retired iframe protocol; UI-22 moved it here,
 * named for what it is, when that protocol went.
 *
 * The key mirrors the bikeshed rail/hidden keys in `BikeshedFace`.
 */
export const EDITOR_FONT_SIZE_KEY = 'tandem.bikeshed.font-size'
export const EDITOR_FONT_SIZE_DEFAULT = 16
export const EDITOR_FONT_SIZE_MIN = 11
export const EDITOR_FONT_SIZE_MAX = 20

type FontStorage = {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
}

function fontStorage(storage?: FontStorage | null): FontStorage | null {
  if (storage !== undefined) return storage
  const candidate =
    typeof globalThis.localStorage !== 'undefined' ? globalThis.localStorage : null
  if (candidate === null) return null
  if (typeof candidate.getItem !== 'function' || typeof candidate.setItem !== 'function')
    return null
  return candidate as FontStorage
}

/** Clamp a candidate size into the sane range (16 for non-numbers). */
export function clampEditorFontSize(px: unknown): number {
  const n = typeof px === 'string' && px.trim() !== '' ? Number(px) : (px as number)
  if (typeof n !== 'number' || !Number.isFinite(n)) return EDITOR_FONT_SIZE_DEFAULT
  return Math.max(EDITOR_FONT_SIZE_MIN, Math.min(EDITOR_FONT_SIZE_MAX, n))
}

/** The stored size, or the 16px default when nothing (or nonsense) is stored. */
export function loadEditorFontSize(storage?: FontStorage | null): number {
  const store = fontStorage(storage)
  if (store === null) return EDITOR_FONT_SIZE_DEFAULT
  return clampEditorFontSize(store.getItem(EDITOR_FONT_SIZE_KEY))
}

/** Persist a posted size, clamped so a stray value cannot stick. */
export function saveEditorFontSize(value: number, storage?: FontStorage | null): void {
  const store = fontStorage(storage)
  if (store === null) return
  store.setItem(EDITOR_FONT_SIZE_KEY, String(clampEditorFontSize(value)))
}
