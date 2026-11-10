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

/**
 * Wrap the selection as `[label](url)` and select the literal placeholder, so
 * typing replaces it. A collapsed caret inserts `[](url)` with the caret
 * between the brackets. Every other byte of the block is untouched.
 */
export function linkInline(source: string, selection: InlineSelection): InlineCommandResult {
  const start = Math.max(0, Math.min(source.length, Math.min(selection.from, selection.to)))
  const end = Math.max(0, Math.min(source.length, Math.max(selection.from, selection.to)))
  if (start === end) {
    return {
      source: source.slice(0, start) + '[](url)' + source.slice(end),
      selection: { from: start + 1, to: start + 1 },
    }
  }
  const label = source.slice(start, end)
  const placeholder = start + 1 + label.length + 2
  return {
    source: source.slice(0, start) + `[${label}](url)` + source.slice(end),
    selection: { from: placeholder, to: placeholder + 3 },
  }
}

/**
 * One inline action: its label and title, the style it toggles, and the key
 * that names it (null for strikethrough, which has no shortcut). The keymap
 * and the bar read this one table, so a key and its button cannot drift.
 */
export type InlineAction = {
  id: string
  style: InlineStyle
  key: string | null
  label: string
  title: string
}

export const INLINE_ACTIONS: readonly InlineAction[] = [
  { id: 'bold', style: 'bold', key: 'b', label: 'Bold', title: 'Bold' },
  { id: 'italic', style: 'italic', key: 'i', label: 'Italic', title: 'Italic' },
  {
    id: 'strikethrough',
    style: 'strikethrough',
    key: null,
    label: 'Strikethrough',
    title: 'Strikethrough',
  },
  { id: 'inline-code', style: 'code', key: 'e', label: 'Inline code', title: 'Inline code' },
]

/** The action a key press names, or null when it is not a plain modifier press. */
export function inlineAction(input: {
  key: string
  mod: boolean
  shift: boolean
  alt: boolean
}): InlineAction | null {
  if (!input.mod || input.shift || input.alt) return null
  const key = input.key.toLowerCase()
  return INLINE_ACTIONS.find((action) => action.key === key) ?? null
}

/** The style a key press toggles, or null when it is not a plain modifier press. */
export function inlineKey(input: {
  key: string
  mod: boolean
  shift: boolean
  alt: boolean
}): InlineStyle | null {
  return inlineAction(input)?.style ?? null
}
