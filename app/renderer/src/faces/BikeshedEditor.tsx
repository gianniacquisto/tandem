import { useEffect, useRef, useState } from 'react'

import {
  editorCall,
  editorDocument,
  editorMode,
  loadEditorFontSize,
  parseEditorMessage,
  saveEditorFontSize,
  type BikeshedMode,
  type EditorMessage,
} from '../shell/editor'

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
  onMessage: (message: EditorMessage) => void
  /** Preview is the same document read-only; Edit is writable. */
  mode: BikeshedMode
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
 *
 * The face's breadcrumb carries the note's name, the saving word and the mode
 * control; this component owns the frame and the mode it is put in.
 */
export function BikeshedEditor({
  path,
  text,
  token,
  noteIndex,
  onMessage,
  mode,
}: BikeshedEditorProps) {
  const frame = useRef<HTMLIFrameElement | null>(null)
  const listener = useRef(onMessage)
  listener.current = onMessage
  const [ready, setReady] = useState(false)
  const modeRef = useRef(mode)
  modeRef.current = mode

  function post(call: { type: string; name: string; args: unknown[] }): void {
    frame.current?.contentWindow?.postMessage(call, '*')
  }

  useEffect(() => {
    function receive(event: MessageEvent): void {
      if (event.source !== frame.current?.contentWindow) return
      const message = parseEditorMessage(event.data)
      if (message === null) return
      if (message.type === 'loaded') {
        // The persisted size goes to the page with the index: the bundle
        // holds no persistence of its own, so a relaunch would otherwise
        // reset the toolbar's A-/A+ control to 16px.
        frame.current?.contentWindow?.postMessage(
          editorCall('setFontSize', [loadEditorFontSize()]),
          '*',
        )
        setReady(true)
      }
      // The toolbar posts every step; the renderer keeps it, so the next
      // load restores it. The bundle never echoes its own setFontSize.
      if (message.type === 'font-size') saveEditorFontSize(message.value)
      listener.current(message)
    }
    window.addEventListener('message', receive)
    return () => window.removeEventListener('message', receive)
  }, [])

  useEffect(() => {
    if (!ready) return
    frame.current?.contentWindow?.postMessage(editorCall('setNoteIndex', [noteIndex]), '*')
  }, [noteIndex, ready])

  // Preview is the same document read-only: entering Edit focuses the
  // editor; the face's breadcrumb keeps the focus on entering Preview.
  useEffect(() => {
    if (!ready || path === '') return
    post(editorMode(mode))
    if (mode === 'edit') {
      post(editorCall('focusEditor'))
    }
    // `mode` is the view revision: the document stays, only readOnly flips.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, ready, path])

  useEffect(() => {
    if (!ready || path === '') return
    frame.current?.contentWindow?.postMessage(editorDocument(path, text), '*')
    // A new document opens ready to type only in Edit; Preview keeps the
    // focus it already has (the breadcrumb toggle, or the tree that opened
    // the note).
    if (modeRef.current === 'edit') {
      frame.current?.contentWindow?.postMessage(editorCall('focusEditor'), '*')
    }
    // `token` is the document revision: a new token replaces the document; a
    // changed `text` is the human's own typing and must not reset the caret.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token, ready, path])

  return (
    <section className="editor" aria-label="Note editor">
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
