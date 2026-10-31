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
