/**
 * The editor iframe's protocol, as data (`Editor/src/bridge.js` and
 * `EditorHost.swift` in the Swift app).
 *
 * The committed bundle still speaks to `window.webkit.messageHandlers.tandem`;
 * the frame installs a shim that forwards the same JSON to its parent. These
 * functions parse what the frame posts and build what the parent posts back,
 * so the contract is testable without a browser and the renderer owns no
 * second spelling of it.
 */
export type EditorMessage =
  | { type: 'loaded' }
  | { type: 'dirty'; text: string }
  | { type: 'link-opened'; target: string }
  | { type: 'checkbox-toggled'; line: number; checked: boolean }
  | { type: 'send-to-todos'; text: string }
  | { type: 'font-size'; value: number }
  | { type: 'toggle-mode' }

/** One call of a page global, the frame's replacement for `callAsyncJavaScript`. */
export type EditorCall = {
  type: 'tandem-editor-call'
  name: string
  args: unknown[]
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function parse(raw: unknown): unknown {
  if (typeof raw !== 'string') return raw
  try {
    return JSON.parse(raw) as unknown
  } catch {
    return null
  }
}

/**
 * A message the bundle posted, or null for anything else.
 *
 * A malformed or half message is dropped rather than guessed at: a `dirty`
 * without its text must never become an empty document, which would be a lost
 * note.
 */
export function parseEditorMessage(raw: unknown): EditorMessage | null {
  const value = parse(raw)
  if (!record(value)) return null
  switch (value['type']) {
    case 'loaded':
      return { type: 'loaded' }
    case 'dirty':
      return typeof value['text'] === 'string' ? { type: 'dirty', text: value['text'] } : null
    case 'link-opened':
      return typeof value['target'] === 'string'
        ? { type: 'link-opened', target: value['target'] }
        : null
    case 'checkbox-toggled':
      return typeof value['line'] === 'number' && typeof value['checked'] === 'boolean'
        ? { type: 'checkbox-toggled', line: value['line'], checked: value['checked'] }
        : null
    case 'send-to-todos':
      return typeof value['text'] === 'string'
        ? { type: 'send-to-todos', text: value['text'] }
        : null
    case 'font-size':
      return typeof value['value'] === 'number'
        ? { type: 'font-size', value: value['value'] }
        : null
    case 'toggle-mode':
      return { type: 'toggle-mode' }
    default:
      return null
  }
}

/** One page global, called with its arguments. */
export function editorCall(name: string, args: unknown[] = []): EditorCall {
  return { type: 'tandem-editor-call', name, args }
}

/** `setDocument(markdown, notePath)`: the whole document and the note it is. */
export function editorDocument(path: string, text: string): EditorCall {
  return editorCall('setDocument', [text, path])
}

/**
 * The persisted note font size (the toolbar's A-/A+ control).
 *
 * The bundle owns the stepping and the CSS property (`Editor/src/main.js`:
 * 16px default, 11-20px range, persisted by the host); the renderer owns
 * the persistence, so a relaunch restores the size and the frame is told
 * it on `loaded`. The key mirrors the bikeshed rail/hidden keys in
 * `BikeshedFace`.
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

/** Persist a toolbar-posted size, clamped so a stray post cannot stick. */
export function saveEditorFontSize(value: number, storage?: FontStorage | null): void {
  const store = fontStorage(storage)
  if (store === null) return
  store.setItem(EDITOR_FONT_SIZE_KEY, String(clampEditorFontSize(value)))
}

/**
 * The Preview, Edit and Write modes (the titlebar segmented control).
 *
 * The bundle owns the readOnly/editable switch (`Editor/src/main.js`:
 * `setMode`); the renderer owns the persistence and the default rule, so a
 * relaunch restores the last choice and the frame is told it on `loaded`.
 * Write is the renderer's own block surface and never reaches the bundle.
 * The key sits beside the rail/hidden/font-size keys in `BikeshedFace`.
 * UI-21 renames Edit to Source and retires Preview; this ticket adds Write.
 */
export type BikeshedMode = 'preview' | 'edit' | 'write'

export const BIKESHED_MODE_KEY = 'tandem.bikeshed.mode'
export const BIKESHED_MODE_DEFAULT: BikeshedMode = 'preview'

function isMode(value: unknown): value is BikeshedMode {
  return value === 'preview' || value === 'edit' || value === 'write'
}

/** The stored choice, or the preview default when nothing (or nonsense) is stored. */
export function loadBikeshedMode(storage?: FontStorage | null): BikeshedMode {
  const store = fontStorage(storage)
  if (store === null) return BIKESHED_MODE_DEFAULT
  const raw = store.getItem(BIKESHED_MODE_KEY)
  return isMode(raw) ? raw : BIKESHED_MODE_DEFAULT
}

/** Persist the titlebar control's choice, ignoring nonsense so it cannot stick. */
export function saveBikeshedMode(mode: BikeshedMode, storage?: FontStorage | null): void {
  const store = fontStorage(storage)
  if (store === null) return
  if (!isMode(mode)) return
  store.setItem(BIKESHED_MODE_KEY, mode)
}

/**
 * The mode a newly opened note starts in: an empty note (every UI-1/UI-4
 * creation) always opens in Write ready to type, regardless of the stored
 * choice; otherwise the stored choice wins (Preview by default until UI-21).
 */
export function defaultBikeshedMode(stored: unknown, text: string): BikeshedMode {
  if (text.trim() === '') return 'write'
  return isMode(stored) ? stored : 'preview'
}

/** `setMode(mode)`: the preview/edit switch the committed bundle reads. */
export function editorMode(mode: BikeshedMode): EditorCall {
  return editorCall('setMode', [mode])
}
