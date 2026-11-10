import { useLayoutEffect, useRef } from 'react'

/**
 * The Source surface: the open note's exact markdown in one plain textarea.
 *
 * Not a second editor and not a decorated view. Its value is exact bytes, so a
 * note that only passes through Source cannot be rewritten by the act of
 * viewing it. Every input goes through the face's `applyDirty`, the same dirty
 * path the block surface uses, so autosave, the unmount flush and the conflict
 * check see the newest text.
 *
 * A textarea's selection is already a document offset, which is the unit the
 * switch carries: the caret arrives as an offset, the surface places it and
 * scrolls it into view, and reports its own selection in the same terms.
 */
export type SourceSurfaceProps = {
  /** The buffer's exact markdown; never a normalised form. */
  text: string
  /** The document offset the switch carried, or null when there is none. */
  caret: number | null
  /** Changes when the note is opened or adopted, so the surface takes focus. */
  focusToken: number
  /** The same dirty path the block surface uses. */
  onChange: (text: string) => void
  /** The current caret as an offset into the note. */
  onCaret: (offset: number) => void
}

/** Scroll the caret's line into view without measuring a mirror element. */
function reveal(area: HTMLTextAreaElement, offset: number): void {
  const line = area.value.slice(0, offset).split('\n').length - 1
  const style = getComputedStyle(area)
  const size = Number.parseFloat(style.fontSize) || 16
  const lineHeight = Number.parseFloat(style.lineHeight)
  const height = Number.isFinite(lineHeight) ? lineHeight : size * 1.65
  const top = Math.max(0, line - 2) * height
  if (top < area.scrollTop || top > area.scrollTop + area.clientHeight - height) {
    area.scrollTop = top
  }
}

export function SourceSurface({
  text,
  caret,
  focusToken,
  onChange,
  onCaret,
}: SourceSurfaceProps) {
  const area = useRef<HTMLTextAreaElement | null>(null)

  // A switch remounts the surface; the note's open token also reaches here.
  // The offset is the buffer's, so it is clamped to the text that arrived.
  useLayoutEffect(() => {
    const node = area.current
    if (node === null) return
    node.focus()
    const at = Math.max(0, Math.min(node.value.length, caret ?? 0))
    node.setSelectionRange(at, at)
    reveal(node, at)
  }, [focusToken, caret])

  function report(): void {
    const node = area.current
    if (node !== null) onCaret(node.selectionStart)
  }

  return (
    <textarea
      ref={area}
      className="source"
      aria-label="Note source"
      spellCheck={false}
      autoCapitalize="off"
      autoCorrect="off"
      value={text}
      onChange={(event) => {
        onChange(event.target.value)
        onCaret(event.target.selectionStart)
      }}
      onSelect={report}
      onKeyUp={report}
      onClick={report}
    />
  )
}
