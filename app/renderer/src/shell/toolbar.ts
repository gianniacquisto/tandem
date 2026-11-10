import { styleSpans, type InlineStyle } from './inline'
import {
  INLINE_ACTIONS,
  type InlineAction,
} from './inline-commands'
import {
  BLOCK_COMMANDS,
  convertBlock,
  insertLink,
  rowText,
  toggleStyle,
  type CommandTarget,
  type EditorState,
  type SurfaceBlock,
} from './editor-ops'

/**
 * The formatting toolbar's one truth: the actions in the bar's order and the
 * state the caret gives them.
 *
 * Every action dispatches the command the keyboard or the `/` menu already
 * dispatches: the inline rows call UI-19's style toggle, Link is the one new
 * command, the block rows call UI-20's conversion with their target kind, and
 * the heading row walks the cycle from the caret. The inline rows and the
 * keymap read the same `INLINE_ACTIONS` table, so a button and its shortcut
 * cannot drift. Nothing here touches the DOM or React, so the bar's wiring is
 * proved in Node.
 */

export type ToolbarGlyph =
  | 'bold'
  | 'italic'
  | 'strikethrough'
  | 'inline-code'
  | 'link'
  | 'heading'
  | 'bullet-list'
  | 'numbered-list'
  | 'task-list'
  | 'quote'
  | 'code-block'

export type ToolbarAction = {
  type: 'action'
  id: string
  label: string
  title: string
  glyph: ToolbarGlyph
  group: 'inline' | 'block'
  /** The inline style a key names, when this row toggles one. */
  style?: InlineStyle
  /** The key that dispatches this row, when it has one. */
  key?: string | null
  /** The `/` menu target this row converts to, when it has one. */
  target?: CommandTarget
  /** The same inline action the keymap returns, so the two cannot drift. */
  inline?: InlineAction
  command: (state: EditorState) => EditorState
}

export type ToolbarSeparator = { type: 'separator' }

export type ToolbarRow = ToolbarAction | ToolbarSeparator

const INLINE_GLYPHS: Record<InlineStyle, ToolbarGlyph> = {
  bold: 'bold',
  italic: 'italic',
  strikethrough: 'strikethrough',
  code: 'inline-code',
}

const inlineRows: ToolbarAction[] = INLINE_ACTIONS.map((action) => ({
  type: 'action',
  id: action.id,
  label: action.label,
  title: action.title,
  glyph: INLINE_GLYPHS[action.style],
  group: 'inline',
  style: action.style,
  key: action.key,
  inline: action,
  command: (state) => toggleStyle(state, action.style),
}))

const linkRow: ToolbarAction = {
  type: 'action',
  id: 'link',
  label: 'Link',
  title: 'Link',
  glyph: 'link',
  group: 'inline',
  command: insertLink,
}

/** The cycle the heading row walks, from the focused block. */
export function headingTarget(surface: SurfaceBlock | undefined): CommandTarget {
  if (surface !== undefined && surface.kind === 'heading') {
    if (surface.level === 1) return { kind: 'heading', level: 2 }
    if (surface.level === 2) return { kind: 'heading', level: 3 }
    return { kind: 'paragraph' }
  }
  return { kind: 'heading', level: 1 }
}

const headingRow: ToolbarAction = {
  type: 'action',
  id: 'heading',
  label: 'Heading',
  title: 'Heading (cycles H1 \u2192 H2 \u2192 H3 \u2192 body)',
  glyph: 'heading',
  group: 'block',
  command: (state) =>
    convertBlock(
      state,
      state.selection.head.block,
      headingTarget(state.blocks[state.selection.head.block]),
    ),
}

const BLOCK_ROWS: Array<{
  id: string
  commandId: string
  title: string
  glyph: ToolbarGlyph
}> = [
  { id: 'bullet-list', commandId: 'bulleted', title: 'Bullet list', glyph: 'bullet-list' },
  { id: 'numbered-list', commandId: 'numbered', title: 'Numbered list', glyph: 'numbered-list' },
  { id: 'task-list', commandId: 'task', title: 'Task list', glyph: 'task-list' },
  { id: 'quote', commandId: 'quote', title: 'Quote', glyph: 'quote' },
  { id: 'code-block', commandId: 'code', title: 'Code block', glyph: 'code-block' },
]

const blockRows: ToolbarAction[] = BLOCK_ROWS.map((row) => {
  const command = BLOCK_COMMANDS.find((entry) => entry.id === row.commandId)
  if (command === undefined) throw new Error(`No block command is named ${row.commandId}.`)
  return {
    type: 'action',
    id: row.id,
    label: row.title,
    title: row.title,
    glyph: row.glyph,
    group: 'block',
    target: command.target,
    command: (state) => convertBlock(state, state.selection.head.block, command.target),
  }
})

/** The bar's actions in order: the inline group, Link, heading, then blocks. */
export const TOOLBAR_ROWS: readonly ToolbarRow[] = [
  ...inlineRows,
  linkRow,
  headingRow,
  { type: 'separator' },
  ...blockRows,
]

const NO_PRESSED: ReadonlySet<string> = new Set()

export type ToolbarStatus = { disabled: boolean; pressed: ReadonlySet<string> }

function isText(surface: SurfaceBlock): boolean {
  return (
    surface.kind === 'paragraph' ||
    surface.kind === 'heading' ||
    surface.kind === 'list' ||
    surface.kind === 'quote' ||
    surface.kind === 'task'
  )
}

/** A surface the bar's actions can act on: every editable kind but a card. */
function isEditable(surface: SurfaceBlock | undefined): surface is SurfaceBlock {
  if (surface === undefined) return false
  return surface.kind !== 'opaque' && surface.kind !== 'image' && surface.kind !== 'rule'
}

/** Whether the selection is wholly covered by a style's spans. */
function coversStyle(text: string, style: InlineStyle, from: number, to: number): boolean {
  const spans = styleSpans(text, style)
  if (from === to) {
    return spans.some((span) => from >= span.from && from <= span.to)
  }
  let cursor = from
  const ordered = spans
    .filter((span) => span.to > from && span.from < to)
    .sort((a, b) => a.from - b.from)
  for (const span of ordered) {
    if (span.from > cursor) return false
    cursor = Math.max(cursor, span.to)
    if (cursor >= to) return true
  }
  return false
}

/**
 * What the bar shows for one caret and focus state: the pressed actions and
 * whether the whole row is inert. A caret in an opaque read-only card, or a
 * surface that lost focus to another control, disables every button.
 */
export function toolbarStatus(state: EditorState, focused: boolean): ToolbarStatus {
  const surface = state.blocks[state.selection.head.block]
  if (!focused || !isEditable(surface)) return { disabled: true, pressed: NO_PRESSED }

  const pressed = new Set<string>()
  const head = state.selection.head
  const anchor = state.selection.anchor
  if (isText(surface) && anchor.block === head.block && anchor.row === head.row) {
    const text = rowText(surface, head.row)
    const from = Math.min(anchor.offset, head.offset)
    const to = Math.max(anchor.offset, head.offset)
    for (const action of INLINE_ACTIONS) {
      if (coversStyle(text, action.style, from, to)) pressed.add(action.id)
    }
  }

  const level = surface.kind === 'heading' ? (surface.level ?? 0) : 0
  if (surface.kind === 'heading' && level >= 1 && level <= 3) {
    pressed.add('heading')
  } else if (surface.kind === 'list' && surface.ordered) {
    pressed.add('numbered-list')
  } else if (surface.kind === 'list') {
    pressed.add('bullet-list')
  } else if (surface.kind === 'task') {
    pressed.add('task-list')
  } else if (surface.kind === 'quote') {
    pressed.add('quote')
  } else if (surface.kind === 'code') {
    pressed.add('code-block')
  }

  return { disabled: false, pressed }
}
