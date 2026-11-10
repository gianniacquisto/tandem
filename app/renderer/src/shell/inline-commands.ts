import { styleSpans, type InlineStyle } from './inline'

export type { InlineStyle } from './inline'

/**
 * The three inline commands (Mod+B, Mod+I, Mod+E), as pure edits over one
 * block's source and the selection inside it.
 *
 * A command only ever changes the touched range: the block's other bytes stay
 * as written, so the surface marks the block dirty and the autosave and the
 * conflict banner see an ordinary keystroke. A style already covering the
 * selection is removed; a style covering only part of it wraps the whole
 * selection. A collapsed caret inserts an empty pair with the caret between
 * the delimiters, and a second press removes that pair.
 */

export const INLINE_DELIMITER: Record<InlineStyle, string> = {
  bold: '**',
  italic: '*',
  strikethrough: '~~',
  code: '`',
}

export type InlineSelection = { from: number; to: number }

export type InlineCommandResult = { source: string; selection: InlineSelection }

export function toggleInlineStyle(
  source: string,
  selection: InlineSelection,
  style: InlineStyle,
): InlineCommandResult {
  const delimiter = INLINE_DELIMITER[style]
  const start = Math.max(0, Math.min(source.length, Math.min(selection.from, selection.to)))
  const end = Math.max(0, Math.min(source.length, Math.max(selection.from, selection.to)))

  if (start === end) {
    // A second press removes the empty pair it inserted, caret between.
    if (
      source.slice(start - delimiter.length, start) === delimiter &&
      source.slice(start, start + delimiter.length) === delimiter
    ) {
      const caret = start - delimiter.length
      return {
        source: source.slice(0, caret) + source.slice(start + delimiter.length),
        selection: { from: caret, to: caret },
      }
    }

    const spanned = styleSpans(source, style).find(
      (span) => start >= span.from && start <= span.to,
    )
    if (spanned !== undefined) {
      const before = spanned.from - spanned.marker.length
      const after = spanned.to + spanned.marker.length
      return {
        source: source.slice(0, before) + source.slice(spanned.from, spanned.to) + source.slice(after),
        selection: { from: start - spanned.marker.length, to: start - spanned.marker.length },
      }
    }

    return {
      source: source.slice(0, start) + delimiter + delimiter + source.slice(start),
      selection: { from: start + delimiter.length, to: start + delimiter.length },
    }
  }

  const covering = styleSpans(source, style).find(
    (span) => span.from <= start && span.to >= end,
  )
  if (covering !== undefined) {
    const before = covering.from - covering.marker.length
    const after = covering.to + covering.marker.length
    return {
      source: source.slice(0, before) + source.slice(covering.from, covering.to) + source.slice(after),
      selection: {
        from: start - covering.marker.length,
        to: end - covering.marker.length,
      },
    }
  }

  return {
    source:
      source.slice(0, start) + delimiter + source.slice(start, end) + delimiter + source.slice(end),
    selection: { from: start + delimiter.length, to: end + delimiter.length },
  }
}

/** The command a key press names, or null when it is not a plain modifier press. */
export function inlineKey(input: {
  key: string
  mod: boolean
  shift: boolean
  alt: boolean
}): InlineStyle | null {
  if (!input.mod || input.shift || input.alt) return null
  switch (input.key.toLowerCase()) {
    case 'b':
      return 'bold'
    case 'i':
      return 'italic'
    case 'e':
      return 'code'
    default:
      return null
  }
}
