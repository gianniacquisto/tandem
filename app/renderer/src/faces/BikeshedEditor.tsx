import { useEffect, useRef, useState } from 'react'

import { editorCall, editorDocument, parseEditorMessage, type EditorMessage } from '../shell/editor'

/** The frame's document, beside the built page in both dev and packaged runs. */
function frameUrl(): string {
  return new URL('editor-frame.html', window.location.href).toString()
}

export type BikeshedEditorProps = {
  /** The open note's path, or the empty string when nothing is open. */
  path: string
  /** The markdown to hand the page on a document change (open or adopt). */
  text: string
  /** Increments when the page must replace its document; a keystroke does not. */
  token: number
  /** Every note path, the flat list the committed bundle's resolver reads. */
  noteIndex: string[]
  saving: boolean
  onMessage: (message: EditorMessage) => void
}

/**
 * The commit's CodeMirror bundle in its one same-origin frame.
 *
 * The bundle is loaded byte for byte from `public/editor/`; the frame supplies
 * the `window.webkit` handler it was written against and listens for the page
 * globals the Swift host called. Note text never leaves this frame except as
 * the `dirty` message's markdown, and the frame holds no bridge of its own: the
 * only call surface is `window.tandem` in the parent, which this frame never
 * touches.
 */
export function BikeshedEditor({
  path,
  text,
  token,
  noteIndex,
  saving,
  onMessage,
}: BikeshedEditorProps) {
  const frame = useRef<HTMLIFrameElement | null>(null)
  const listener = useRef(onMessage)
  listener.current = onMessage
  const [ready, setReady] = useState(false)

  useEffect(() => {
    function receive(event: MessageEvent): void {
      if (event.source !== frame.current?.contentWindow) return
      const message = parseEditorMessage(event.data)
      if (message === null) return
      if (message.type === 'loaded') setReady(true)
      listener.current(message)
    }
    window.addEventListener('message', receive)
    return () => window.removeEventListener('message', receive)
  }, [])

  useEffect(() => {
    if (!ready) return
    frame.current?.contentWindow?.postMessage(editorCall('setNoteIndex', [noteIndex]), '*')
  }, [noteIndex, ready])

  useEffect(() => {
    if (!ready || path === '') return
    frame.current?.contentWindow?.postMessage(editorDocument(path, text), '*')
    frame.current?.contentWindow?.postMessage(editorCall('focusEditor'), '*')
    // `token` is the document revision: a new token replaces the document; a
    // changed `text` is the human's own typing and must not reset the caret.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token, ready, path])

  const folder = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : ''
  const title = (path.split('/').pop() ?? '').replace(/\.md$/i, '')

  return (
    <section className="editor" aria-label="Note editor">
      <header className="editor__titlebar">
        <span className="editor__folder">{folder === '' ? 'Bikeshed' : folder}</span>
        <span className="editor__title">{title === '' ? 'No note selected' : title}</span>
        {saving ? <span className="editor__saving">saving{'\u2026'}</span> : null}
      </header>
      {path === '' ? (
        <div className="editor__empty">
          <p className="editor__empty-title">No note selected</p>
          <p className="editor__empty-body">Pick a note in the tree.</p>
        </div>
      ) : (
        <iframe
          ref={frame}
          className="editor__frame"
          title="Note text"
          src={frameUrl()}
          sandbox="allow-scripts allow-same-origin"
        />
      )}
    </section>
  )
}
