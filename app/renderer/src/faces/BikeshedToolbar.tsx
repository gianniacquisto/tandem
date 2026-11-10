import type { ReactNode } from 'react'

import type { EditorState } from '../shell/editor-ops'
import {
  TOOLBAR_ROWS,
  toolbarStatus,
  type ToolbarAction,
  type ToolbarGlyph,
} from '../shell/toolbar'

/**
 * The formatting toolbar: the editor's quiet row above the block surface.
 *
 * Every button names one action from `shell/toolbar.ts` and dispatches the
 * same command the keyboard or the `/` menu dispatches. `mousedown` is
 * prevented so a press never takes the caret or the selection away from the
 * note. The glyphs are the retired bar's line drawings as JSX paths, so no
 * markup string ever reaches the DOM.
 */

export type BikeshedToolbarProps = {
  state: EditorState
  /** The surface holds the caret, so the row may act. */
  focused: boolean
  onRun: (action: ToolbarAction) => void
}

const GLYPHS: Record<ToolbarGlyph, ReactNode> = {
  bold: (
    <>
      <path d="M5 3.5h3.4a2.3 2.3 0 0 1 0 4.6H5z" />
      <path d="M5 8.1h3.9a2.4 2.4 0 0 1 0 4.8H5z" />
    </>
  ),
  italic: <path d="M8 3.5h4M4 12.5h4M9.6 3.5 6.4 12.5" />,
  strikethrough: (
    <>
      <path d="M3.5 8h9" />
      <path d="M10.5 5.2c-.6-.9-1.7-1.4-2.9-1.4-1.6 0-2.7.7-2.7 1.9" />
      <path d="M5.5 10.8c.6.9 1.7 1.4 2.9 1.4 1.6 0 2.7-.7 2.7-1.9" />
    </>
  ),
  'inline-code': <path d="M6 4 3 8l3 4M10 4l3 4-3 4" />,
  link: (
    <>
      <path d="M6.5 9.5 9.5 6.5" />
      <path d="M7.4 4.9 8.9 3.4a2.5 2.5 0 0 1 3.5 3.5l-1.5 1.5" />
      <path d="M8.6 11.1 7.1 12.6a2.5 2.5 0 0 1-3.5-3.5l1.5-1.5" />
    </>
  ),
  heading: <path d="M4 3.5v9M12 3.5v9M4 8h8" />,
  'bullet-list': (
    <>
      <path d="M6.5 4h6.5M6.5 8h6.5M6.5 12h6.5" />
      <circle cx="3" cy="4" r="1" />
      <circle cx="3" cy="8" r="1" />
      <circle cx="3" cy="12" r="1" />
    </>
  ),
  'numbered-list': (
    <>
      <path d="M6.5 4h6.5M6.5 8h6.5M6.5 12h6.5" />
      <path d="M2 3.2 3 2.6V6M1.9 9.9c.3-.4.7-.6 1.1-.6.6 0 1 .4 1 .9 0 1-2 1.1-2 2.4h2" />
    </>
  ),
  'task-list': (
    <>
      <path d="M6.5 4h6.5M6.5 8h6.5M6.5 12h6.5" />
      <rect x="1.6" y="2.6" width="3" height="3" rx="1" />
      <path d="M2.3 4.1l.6.7 1.3-1.5" />
      <rect x="1.6" y="10.6" width="3" height="3" rx="1" />
    </>
  ),
  quote: (
    <>
      <path d="M4 3.5v9" />
      <path d="M7 5h6M7 8h6M7 11h4" />
    </>
  ),
  'code-block': (
    <>
      <rect x="2.5" y="3.5" width="11" height="9" rx="1.5" />
      <path d="M6.6 6.6 5.2 8l1.4 1.4M9.4 6.6 10.8 8l-1.4 1.4" />
    </>
  ),
}

export function BikeshedToolbar({ state, focused, onRun }: BikeshedToolbarProps) {
  const status = toolbarStatus(state, focused)
  return (
    <div
      className="bikeshed__toolbar"
      role="toolbar"
      aria-label="Formatting"
      onMouseDown={(event) => event.preventDefault()}
    >
      {TOOLBAR_ROWS.map((row, index) =>
        row.type === 'separator' ? (
          <span key={`separator-${index}`} className="bikeshed__toolbar-sep" aria-hidden="true" />
        ) : (
          <button
            key={row.id}
            type="button"
            className="bikeshed__toolbar-button"
            aria-label={row.label}
            title={row.title}
            aria-pressed={status.pressed.has(row.id)}
            disabled={status.disabled}
            onClick={() => onRun(row)}
          >
            <svg
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              {GLYPHS[row.glyph]}
            </svg>
          </button>
        ),
      )}
    </div>
  )
}
