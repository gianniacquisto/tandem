/**
 * The editor frame's bridge shim.
 *
 * The committed bundle (`public/editor/editor.js`) posts to
 * `window.webkit.messageHandlers.tandem` and exposes its page globals on
 * `window`, exactly as it did under WKWebView. This frame supplies the handler
 * the browser does not: one `postMessage` to the parent per message the bundle
 * sent, and one page global per call the parent posts back. Nothing else
 * crosses, and the frame never reads the parent's window.
 */
type EditorMessage = { type: string } & Record<string, unknown>

type ParentCall = { type: 'tandem-editor-call'; name: string; args: unknown[] }

const parentWindow = window.parent

function post(message: EditorMessage): void {
  // '*' is safe here: the parent accepts only its own frame's window as the
  // source, and this frame accepts only its own parent's, on both sides.
  parentWindow.postMessage(message, '*')
}

/** The WKWebView handler the bundle was written against. */
const webkit = {
  messageHandlers: {
    tandem: {
      postMessage: (raw: string) => post(JSON.parse(raw) as EditorMessage),
    },
  },
}
;(window as unknown as { webkit: typeof webkit }).webkit = webkit

window.addEventListener('message', (event: MessageEvent) => {
  if (event.source !== parentWindow) return
  const call = event.data as ParentCall | null
  if (call === null || typeof call !== 'object' || call.type !== 'tandem-editor-call') return
  const page = window as unknown as Record<string, unknown>
  const fn = page[call.name]
  if (typeof fn === 'function') (fn as (...args: unknown[]) => void).apply(window, call.args)
})

/** The Ligne day/night pairs, the way the Swift host set them on `<html>`. */
function paintTheme(): void {
  const night = window.matchMedia('(prefers-color-scheme: dark)').matches
  document.documentElement.setAttribute('data-theme', night ? 'night' : 'day')
}

paintTheme()
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', paintTheme)

// The bundle self-mounts; the shim must exist before it runs, so it is loaded
// here rather than in a second script tag (module scripts are deferred, classic
// ones are not).
const bundle = document.createElement('script')
bundle.src = './editor/editor.js'
document.body.append(bundle)
