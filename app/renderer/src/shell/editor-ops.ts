import {
  parseDocument,
  serialiseDocument,
  type Block,
  type Document,
  type Part,
} from './blocks'
import { linkInline, toggleInlineStyle, type InlineStyle } from './inline-commands'

/**
 * The pure editing seam under the Write surface.
 *
 * UI-17's `Block` list is the document; this module turns it into the surface's
 * rows, applies one edit at a time, and turns the result back into that same
 * `Document`. Nothing here touches React, the DOM or the browser, so every
 * caret behaviour the ticket names is proved in Node. The surface component is
 * a thin view over these operations; the note's markdown is still produced by
 * UI-17's serialiser, never by a second parser.
 *
 * A `Position` is `{ block, row, offset }`: the top-level block, the row inside
 * it (a list item, a quote line; always 0 for the others) and the caret's
 * character offset in that row. A `Selection` keeps an anchor and a head, the
 * same shape the DOM has.
 */

export type Position = { block: number; row: number; offset: number }

export type Selection = { anchor: Position; head: Position }

/**
 * One editable row. `rest` carries the blocks a composite holds under the row
 * (a nested list, a second paragraph, a nested quote): the surface edits the
 * text and hands the hidden children back untouched, so an edit never flattens
 * the block.
 */
export type SurfaceRow = { text: string; checked?: boolean; rest?: Block[] }

type Base = {
  /** The gap UI-17 placed before this block, kept so an untouched note round-trips. */
  gap: string
  /** The block's exact source bytes, as UI-17 read them. */
  source: string
  edited: boolean
  origin: Block
}

export type TextSurface = Base & {
  kind: 'paragraph' | 'heading' | 'list' | 'quote' | 'task'
  rows: SurfaceRow[]
  level?: number
  ordered?: boolean
  start?: number
}

export type CodeSurface = Base & {
  kind: 'code'
  language: string | null
  text: string
}

export type ImageSurface = Base & {
  kind: 'image'
  alt: string
  destination: string
  title: string | null
}

export type MathSurface = Base & { kind: 'math'; text: string }

export type RuleSurface = Base & { kind: 'rule' }

export type OpaqueSurface = Base & { kind: 'opaque' }

export type SurfaceBlock =
  | TextSurface
  | CodeSurface
  | ImageSurface
  | MathSurface
  | RuleSurface
  | OpaqueSurface

export type EditorState = {
  blocks: SurfaceBlock[]
  trailingGap: string
  selection: Selection
}

export type History = {
  past: EditorState[]
  future: EditorState[]
  /** The last recorded change was a typing run, so the next keystroke joins it. */
  merging: boolean
}

const HISTORY_LIMIT = 100

export const emptyHistory: History = { past: [], future: [], merging: false }

function start(): Position {
  return { block: 0, row: 0, offset: 0 }
}

function taskOf(block: Block | undefined): Extract<Block, { kind: 'task' }> | null {
  return block !== undefined && block.kind === 'task' ? block : null
}

function paragraphText(block: Block | undefined): string {
  return block !== undefined && block.kind === 'paragraph' ? block.text : ''
}

function surfaceOf(block: Block, gap: string): SurfaceBlock {
  const base: Base = { gap, source: block.source, edited: block.edited, origin: block }
  switch (block.kind) {
    case 'paragraph':
      return { ...base, kind: 'paragraph', rows: [{ text: block.text }] }
    case 'heading':
      return { ...base, kind: 'heading', level: block.level, rows: [{ text: block.text }] }
    case 'list': {
      const tasks = block.items.length > 0 && block.items.every((item) => taskOf(item[0]) !== null)
      if (tasks) {
        return {
          ...base,
          kind: 'task',
          rows: block.items.map((item) => {
            const task = taskOf(item[0])
            const rest = [...(task?.blocks.slice(1) ?? []), ...item.slice(1)]
            return {
              text: paragraphText(task?.blocks[0]),
              checked: task?.checked === true,
              ...(rest.length > 0 ? { rest } : {}),
            }
          }),
        }
      }
      return {
        ...base,
        kind: 'list',
        ordered: block.ordered,
        start: block.start,
        rows: block.items.map((item) => {
          // An item whose first block is not a paragraph keeps every block;
          // the visible row is empty and the children ride along in `rest`.
          const first = item[0]
          const rest = first?.kind === 'paragraph' ? item.slice(1) : item
          return {
            text: paragraphText(first),
            ...(rest.length > 0 ? { rest } : {}),
          }
        }),
      }
    }
    case 'task': {
      const rest = block.blocks.slice(1)
      return {
        ...base,
        kind: 'task',
        rows: [
          {
            text: paragraphText(block.blocks[0]),
            checked: block.checked,
            ...(rest.length > 0 ? { rest } : {}),
          },
        ],
      }
    }
    case 'quote': {
      const rows: SurfaceRow[] = []
      for (const child of block.blocks) {
        if (child.kind === 'paragraph') {
          for (const text of child.text.split('\n')) rows.push({ text })
        } else {
          // A non-paragraph child (a nested list or quote) follows the row it
          // sat after; with no paragraph before it, it gets an empty row so
          // the order is kept.
          if (rows.length === 0) rows.push({ text: '' })
          const last = rows[rows.length - 1]!
          last.rest = [...(last.rest ?? []), child]
        }
      }
      return { ...base, kind: 'quote', rows }
    }
    case 'code':
      return { ...base, kind: 'code', language: block.language, text: block.text }
    case 'image':
      return {
        ...base,
        kind: 'image',
        alt: block.alt,
        destination: block.destination,
        title: block.title,
      }
    case 'rule':
      return { ...base, kind: 'rule' }
    case 'math':
      return { ...base, kind: 'math', text: block.text }
    case 'opaque':
      return { ...base, kind: 'opaque' }
  }
}

/** The surface's state for an opened note. An empty note gets one paragraph. */
export function fromDocument(document: Document): EditorState {
  const blocks: SurfaceBlock[] = []
  let gap = ''
  for (const part of document.parts) {
    if (part.type === 'gap') {
      gap += part.source
      continue
    }
    blocks.push(surfaceOf(part.block, gap))
    gap = ''
  }
  if (blocks.length === 0) {
    const empty: Block = { kind: 'paragraph', text: '', source: '', edited: false }
    blocks.push(surfaceOf(empty, ''))
  }
  return { blocks, trailingGap: gap, selection: { anchor: start(), head: start() } }
}

/**
 * Commit one block's raw markdown back into the surface.
 *
 * The text is parsed through UI-17's document model, so an opaque table that a
 * human rewrites into prose comes back as the prose's blocks and one that stays
 * a table comes back opaque with its new bytes. The edited block's own gap is
 * kept before the first replacement, the parsed trailing whitespace lands on
 * the next block, and the caret returns to the replacement's start.
 */
export function commitBlockSource(
  state: EditorState,
  index: number,
  source: string,
): EditorState {
  const target = state.blocks[index]
  if (target === undefined) throw new RangeError(`no block at index ${index}`)
  const parsed = fromDocument(parseDocument(source))
  const replacement = parsed.blocks
  replacement[0] = { ...replacement[0]!, gap: target.gap + replacement[0]!.gap }
  const next = state.blocks[index + 1]
  const blocks = [...state.blocks]
  blocks.splice(index, 1, ...replacement)
  let trailingGap = state.trailingGap
  if (next !== undefined) {
    blocks[index + replacement.length] = {
      ...next,
      gap: parsed.trailingGap + next.gap,
    }
  } else {
    trailingGap = parsed.trailingGap + trailingGap
  }
  const head = { block: index, row: 0, offset: 0 }
  return { blocks, trailingGap, selection: { anchor: head, head } }
}

const PARAGRAPH_MARKER = /^(#{1,6}(\s|$)|[-*+](\s|$)|\d+[.)](\s|$)|>(\s|$)|```|~~~|(?:-{3,}|\*{3,}|_{3,})\s*$)/

/** A paragraph that would parse as a construct is escaped so it reopens as a paragraph. */
function escapeParagraph(text: string): string {
  return PARAGRAPH_MARKER.test(text) ? `\\${text}` : text
}

function paragraph(text: string): Block {
  return { kind: 'paragraph', text, source: '', edited: true }
}

/** A task row's children: its edited paragraph, then its hidden nested blocks. */
function taskChildBlocks(row: SurfaceRow): Block[] {
  const rest = row.rest ?? []
  return row.text === '' ? rest : [paragraph(row.text), ...rest]
}

function blockOf(surface: SurfaceBlock): Block {
  switch (surface.kind) {
    case 'paragraph':
      return { kind: 'paragraph', text: escapeParagraph(rowText(surface, 0)), source: '', edited: true }
    case 'heading':
      return {
        kind: 'heading',
        level: Math.min(3, Math.max(1, surface.level ?? 1)),
        text: rowText(surface, 0),
        source: '',
        edited: true,
      }
    case 'list': {
      const items = surface.rows.map((row) => [paragraph(row.text), ...(row.rest ?? [])])
      return {
        kind: 'list',
        ordered: surface.ordered === true,
        start: surface.start ?? 1,
        items,
        source: '',
        edited: true,
      }
    }
    case 'task': {
      const tasks = surface.rows.map((row) => ({
        kind: 'task' as const,
        checked: row.checked === true,
        blocks: taskChildBlocks(row),
        source: '',
        edited: true,
      }))
      if (surface.origin.kind === 'task') {
        const only = tasks[0]
        if (only !== undefined) return only
      }
      return { kind: 'list', ordered: false, start: 1, items: tasks.map((task) => [task]), source: '', edited: true }
    }
    case 'quote':
      return {
        kind: 'quote',
        blocks: surface.rows.flatMap((row) => {
          const rest = row.rest ?? []
          if (row.text === '' && rest.length > 0) return rest
          return [paragraph(row.text), ...rest]
        }),
        source: '',
        edited: true,
      }
    case 'code':
      return { kind: 'code', language: surface.language, text: surface.text, source: '', edited: true }
    case 'image':
      return {
        kind: 'image',
        alt: surface.alt,
        destination: surface.destination,
        title: surface.title,
        source: '',
        edited: true,
      }
    case 'rule':
      return { kind: 'rule', source: '', edited: true }
    case 'math':
      return { kind: 'math', display: true, text: surface.text, source: '', edited: true }
    case 'opaque':
      throw new Error('an opaque block has no canonical rendering')
  }
}

/** The surface state as UI-17's document, gaps and untouched sources included. */
export function toDocument(state: EditorState): Document {
  const parts: Part[] = []
  for (const surface of state.blocks) {
    if (surface.gap !== '') parts.push({ type: 'gap', source: surface.gap })
    const block: Block = surface.edited ? blockOf(surface) : surface.origin
    parts.push({ type: 'block', block })
  }
  if (state.trailingGap !== '') parts.push({ type: 'gap', source: state.trailingGap })
  return { parts }
}

/** The note's markdown, through UI-17's one serialiser. */
export function serialise(state: EditorState): string {
  return serialiseDocument(toDocument(state))
}

export function rowText(surface: SurfaceBlock, row: number): string {
  if (surface.kind === 'code' || surface.kind === 'math') return surface.text
  if (
    surface.kind === 'paragraph' ||
    surface.kind === 'heading' ||
    surface.kind === 'list' ||
    surface.kind === 'quote' ||
    surface.kind === 'task'
  ) {
    return surface.rows[row]?.text ?? ''
  }
  return ''
}

export function rowCount(surface: SurfaceBlock): number {
  if (
    surface.kind === 'paragraph' ||
    surface.kind === 'heading' ||
    surface.kind === 'list' ||
    surface.kind === 'quote' ||
    surface.kind === 'task'
  ) {
    return Math.max(1, surface.rows.length)
  }
  return 1
}

function isText(surface: SurfaceBlock): surface is TextSurface {
  return (
    surface.kind === 'paragraph' ||
    surface.kind === 'heading' ||
    surface.kind === 'list' ||
    surface.kind === 'quote' ||
    surface.kind === 'task'
  )
}

function isInert(surface: SurfaceBlock): boolean {
  return surface.kind === 'rule' || surface.kind === 'image' || surface.kind === 'opaque'
}

export function withCaret(state: EditorState, block: number, row: number, offset: number): EditorState {
  const position = clampPosition(state, { block, row, offset })
  return { ...state, selection: { anchor: position, head: position } }
}

export function withSelection(
  state: EditorState,
  anchor: Position,
  head: Position,
): EditorState {
  return {
    ...state,
    selection: { anchor: clampPosition(state, anchor), head: clampPosition(state, head) },
  }
}

function clampPosition(state: EditorState, position: Position): Position {
  const block = Math.max(0, Math.min(state.blocks.length - 1, position.block))
  const surface = state.blocks[block]
  if (surface === undefined) return start()
  const row = Math.max(0, Math.min(rowCount(surface) - 1, position.row))
  const offset = Math.max(0, Math.min(rowText(surface, row).length, position.offset))
  return { block, row, offset }
}

function unordered(state: EditorState): [Position, Position] {
  const { anchor, head } = state.selection
  return compare(anchor, head) <= 0 ? [anchor, head] : [head, anchor]
}

function compare(a: Position, b: Position): number {
  if (a.block !== b.block) return a.block - b.block
  if (a.row !== b.row) return a.row - b.row
  return a.offset - b.offset
}

function collapsed(state: EditorState): boolean {
  const { anchor, head } = state.selection
  return compare(anchor, head) === 0
}

function editBlockAt(state: EditorState, index: number, surface: SurfaceBlock): EditorState {
  const blocks = state.blocks.map((current, i) => (i === index ? { ...surface, edited: true } : current))
  return { ...state, blocks }
}

function replaceBlock(state: EditorState, index: number, surface: SurfaceBlock): SurfaceBlock[] {
  return state.blocks.map((current, i) => (i === index ? { ...surface, edited: true } : current))
}

function clampCaret(state: EditorState, position: Position): Position {
  return clampPosition({ ...state, blocks: state.blocks }, position)
}

/** Where the caret lands after a block before `index` is removed. */
function caretBefore(state: EditorState, index: number): Position {
  const previous = state.blocks[index - 1]
  if (previous !== undefined) {
    const row = rowCount(previous) - 1
    return { block: index - 1, row, offset: rowText(previous, row).length }
  }
  return { block: 0, row: 0, offset: 0 }
}

/** Caret before `first` removed, with the blocks after them now at `first`. */
function removeBlocks(state: EditorState, first: number, count: number): EditorState {
  const caret = caretBefore(state, first)
  const kept = state.blocks.filter((_, i) => i < first || i >= first + count)
  if (kept.length === 0) {
    const empty: Block = { kind: 'paragraph', text: '', source: '', edited: false }
    kept.push(surfaceOf(empty, ''))
  }
  const next: EditorState = { ...state, blocks: kept, selection: { anchor: caret, head: caret } }
  return withCaret(next, caret.block, caret.row, caret.offset)
}

/* ------------------------------------------------------------------ typing */

export function insertText(state: EditorState, text: string): EditorState {
  let current = state
  if (!collapsed(current)) current = deleteSelection(current)
  const position = current.selection.head
  const surface = current.blocks[position.block]
  if (surface === undefined || !isText(surface) && surface.kind !== 'code' && surface.kind !== 'math') return current

  if (surface.kind === 'code' || surface.kind === 'math') {
    const textWithInsert = splice(surface.text, position.offset, text)
    return finish(
      replaceBlockState(current, position.block, { ...surface, text: textWithInsert }),
      position.block,
      0,
      position.offset + text.length,
    )
  }

  const row = surface.rows[position.row]
  if (row === undefined) return current
  const rows = surface.rows.map((item, index) =>
    index === position.row ? { ...item, text: splice(item.text, position.offset, text) } : item,
  )
  return finish(
    replaceBlockState(current, position.block, { ...surface, rows }),
    position.block,
    position.row,
    position.offset + text.length,
  )
}

function splice(text: string, offset: number, insert: string): string {
  return text.slice(0, offset) + insert + text.slice(offset)
}

function replaceBlockState(state: EditorState, index: number, surface: SurfaceBlock): EditorState {
  return { ...state, blocks: replaceBlock(state, index, surface) }
}

function finish(state: EditorState, block: number, row: number, offset: number): EditorState {
  return withCaret(state, block, row, offset)
}

/* ------------------------------------------------------------------- Enter */

export function split(state: EditorState): EditorState {
  const position = state.selection.head
  const surface = state.blocks[position.block]
  if (surface === undefined) return state
  if (surface.kind === 'rule' || surface.kind === 'image' || surface.kind === 'opaque') return state
  if (surface.kind === 'code' || surface.kind === 'math') {
    return finish(
      replaceBlockState(state, position.block, {
        ...surface,
        text: splice(surface.text, position.offset, '\n'),
      }),
      position.block,
      0,
      position.offset + 1,
    )
  }

  const text = surface.rows[position.row]?.text ?? ''
  const before = text.slice(0, position.offset)
  const after = text.slice(position.offset)

  if (surface.kind === 'list' || surface.kind === 'task' || surface.kind === 'quote') {
    if (text === '' && surface.kind !== 'quote') {
      // Enter on an empty item ends the list.
      const rows = surface.rows.filter((_, index) => index !== position.row)
      if (rows.length === 0) return removeBlocks(state, position.block, 1)
      const next = replaceBlockState(state, position.block, { ...surface, rows })
      const row = Math.max(0, position.row - 1)
      return withCaret(next, position.block, row, rowText(next.blocks[position.block]!, row).length)
    }
    const rows = [
      ...surface.rows.slice(0, position.row),
      { ...surface.rows[position.row], text: before },
      { text: after },
      ...surface.rows.slice(position.row + 1),
    ]
    return finish(replaceBlockState(state, position.block, { ...surface, rows }), position.block, position.row + 1, 0)
  }

  const head: SurfaceBlock =
    surface.kind === 'heading'
      ? {
          ...surface,
          rows: [{ text: before }],
        }
      : { ...surface, rows: [{ text: before }] }
  const tail: SurfaceBlock = {
    ...surface,
    kind: 'paragraph',
    level: undefined,
    ordered: undefined,
    start: undefined,
    rows: [{ text: after }],
    gap: '\n',
    source: '',
  }
  const blocks = [
    ...state.blocks.slice(0, position.block),
    { ...head, edited: true },
    { ...tail, edited: true },
    ...state.blocks.slice(position.block + 1),
  ]
  const next: EditorState = { ...state, blocks }
  return finish(next, position.block + 1, 0, 0)
}

/* --------------------------------------------------------------- Backspace */

export function backspace(state: EditorState): EditorState {
  if (!collapsed(state)) return deleteSelection(state)
  const position = state.selection.head
  const surface = state.blocks[position.block]
  if (surface === undefined) return state

  if (surface.kind === 'rule' || surface.kind === 'image') {
    return removeBlocks(state, position.block, 1)
  }
  if (surface.kind === 'opaque') return state

  if (surface.kind === 'code' || surface.kind === 'math') {
    if (position.offset === 0) {
      if (surface.text === '' && position.block > 0) return removeBlocks(state, position.block, 1)
      return position.block === 0 ? state : convertToParagraph(state, position.block, 0)
    }
    return finish(
      replaceBlockState(state, position.block, {
        ...surface,
        text: splice(surface.text, position.offset - 1, ''),
      }),
      position.block,
      0,
      position.offset - 1,
    )
  }

  if (position.offset > 0) {
    const rows = surface.rows.map((item, index) =>
      index === position.row
        ? { ...item, text: splice(item.text, position.offset - 1, '') }
        : item,
    )
    return finish(replaceBlockState(state, position.block, { ...surface, rows }), position.block, position.row, position.offset - 1)
  }

  if (position.row > 0) {
    const previous = surface.rows[position.row - 1]
    const current = surface.rows[position.row]
    if (previous === undefined || current === undefined) return state
    const rest = [...(previous.rest ?? []), ...(current.rest ?? [])]
    const rows = [
      ...surface.rows.slice(0, position.row - 1),
      { ...previous, text: previous.text + current.text, ...(rest.length > 0 ? { rest } : {}) },
      ...surface.rows.slice(position.row + 1),
    ]
    return finish(
      replaceBlockState(state, position.block, { ...surface, rows }),
      position.block,
      position.row - 1,
      previous.text.length,
    )
  }

  return mergeBackward(state, position.block)
}

function mergeBackward(state: EditorState, index: number): EditorState {
  const surface = state.blocks[index]
  if (surface === undefined) return state

  if (blockText(surface) === '' && index > 0) {
    return removeBlocks(state, index, 1)
  }
  if (index === 0) {
    if (surface.kind === 'paragraph') return state
    return convertToParagraph(state, index, 0)
  }

  const previous = state.blocks[index - 1]
  if (previous === undefined) return state
  if (previous.kind === 'paragraph' && surface.kind === 'paragraph') {
    const left = rowText(previous, 0)
    const right = rowText(surface, 0)
    const merged: SurfaceBlock = { ...previous, rows: [{ text: left + right }], edited: true }
    const blocks = state.blocks.filter((_, i) => i !== index).map((block, i) => (i === index - 1 ? merged : block))
    const next: EditorState = { ...state, blocks }
    return finish(next, index - 1, 0, left.length)
  }
  return convertToParagraph(state, index, 0)
}

function blockText(surface: SurfaceBlock): string {
  if (surface.kind === 'code' || surface.kind === 'math') return surface.text
  if (isText(surface)) return surface.rows.map((row) => row.text).join('\n')
  return surface.source
}

function convertToParagraph(state: EditorState, index: number, offset: number): EditorState {
  const surface = state.blocks[index]
  if (surface === undefined) return state
  const text = blockText(surface)
  const paragraph: SurfaceBlock = {
    kind: 'paragraph',
    rows: [{ text }],
    gap: surface.gap,
    source: '',
    edited: true,
    origin: surface.origin,
  }
  // The nested children cannot live in a paragraph, so they follow it as their
  // own blocks rather than disappearing with the marker.
  const rest = isText(surface)
    ? surface.rows.flatMap((row) => row.rest ?? []).map((block) => surfaceOf(block, '\n'))
    : []
  const blocks = [
    ...state.blocks.slice(0, index),
    paragraph,
    ...rest,
    ...state.blocks.slice(index + 1),
  ]
  return finish({ ...state, blocks }, index, 0, offset)
}

/* ---------------------------------------------------------- Delete forward */

export function deleteForward(state: EditorState): EditorState {
  if (!collapsed(state)) return deleteSelection(state)
  const position = state.selection.head
  const surface = state.blocks[position.block]
  if (surface === undefined) return state

  if (surface.kind === 'rule' || surface.kind === 'image') {
    return removeBlocks(state, position.block, 1)
  }
  if (surface.kind === 'opaque') return state

  if (surface.kind === 'code' || surface.kind === 'math') {
    if (position.offset < surface.text.length) {
      return finish(
        replaceBlockState(state, position.block, {
          ...surface,
          text: splice(surface.text, position.offset, ''),
        }),
        position.block,
        0,
        position.offset,
      )
    }
    return mergeForward(state, position.block)
  }

  if (position.offset < rowText(surface, position.row).length) {
    const rows = surface.rows.map((item, index) =>
      index === position.row ? { ...item, text: splice(item.text, position.offset, '') } : item,
    )
    return finish(replaceBlockState(state, position.block, { ...surface, rows }), position.block, position.row, position.offset)
  }

  if (position.row < rowCount(surface) - 1) {
    const current = surface.rows[position.row]
    const next = surface.rows[position.row + 1]
    if (current === undefined || next === undefined) return state
    const rest = [...(current.rest ?? []), ...(next.rest ?? [])]
    const rows = [
      ...surface.rows.slice(0, position.row),
      { ...current, text: current.text + next.text, ...(rest.length > 0 ? { rest } : {}) },
      ...surface.rows.slice(position.row + 2),
    ]
    return finish(replaceBlockState(state, position.block, { ...surface, rows }), position.block, position.row, position.offset)
  }

  return mergeForward(state, position.block)
}

function mergeForward(state: EditorState, index: number): EditorState {
  const surface = state.blocks[index]
  const next = state.blocks[index + 1]
  if (surface === undefined || next === undefined) return state
  if (next.kind === 'rule' || next.kind === 'image') {
    return removeBlocks(state, index + 1, 1)
  }
  if (next.kind === 'opaque' || surface.kind === 'code' || next.kind === 'code' || surface.kind === 'math' || next.kind === 'math') return state
  if (!isText(surface)) return state
  if (surface.kind === 'paragraph' && next.kind === 'paragraph') {
    const left = rowText(surface, 0)
    const right = rowText(next, 0)
    const merged: SurfaceBlock = { ...surface, rows: [{ text: left + right }], edited: true }
    const blocks = state.blocks.filter((_, i) => i !== index + 1).map((block, i) => (i === index ? merged : block))
    return finish({ ...state, blocks }, index, 0, left.length)
  }
  const text = blockText(next)
  const merged: SurfaceBlock = {
    ...surface,
    rows: [{ text: rowText(surface, 0) + text }],
    edited: true,
  }
  const rest = isText(next)
    ? next.rows.flatMap((row) => row.rest ?? []).map((block) => surfaceOf(block, '\n'))
    : []
  const blocks = state.blocks.flatMap((block, i) => {
    if (i === index) return [merged]
    if (i === index + 1) return rest
    return [block]
  })
  return finish({ ...state, blocks }, index, 0, rowText(surface, 0).length)
}

/* ------------------------------------------------------------- selection */

export function selectAll(state: EditorState): EditorState {
  const last = state.blocks.length - 1
  const lastRow = rowCount(state.blocks[last]!) - 1
  return withSelection(state, start(), {
    block: last,
    row: lastRow,
    offset: rowText(state.blocks[last]!, lastRow).length,
  })
}

function markerOf(surface: SurfaceBlock, row: number): string {
  switch (surface.kind) {
    case 'heading':
      return `${'#'.repeat(surface.level ?? 1)} `
    case 'list':
      return surface.ordered === true ? `${(surface.start ?? 1) + row}. ` : '- '
    case 'task':
      return surface.rows[row]?.checked === true ? '- [x] ' : '- [ ] '
    case 'quote':
      return '> '
    default:
      return ''
  }
}

function fullRowSource(surface: SurfaceBlock, row: number): string {
  switch (surface.kind) {
    case 'rule':
      return '---'
    case 'image':
      return `![${surface.alt}](${surface.destination})`
    case 'opaque':
      return surface.source
    case 'code':
      return surface.text
    case 'math':
      return `$$\n${surface.text}\n$$`
    default:
      return markerOf(surface, row) + rowText(surface, row)
  }
}

/** The selected source, markers included for rows selected from their start. */
export function selectedText(state: EditorState): string {
  const [from, to] = unordered(state)
  if (compare(from, to) === 0) return ''
  const lines: string[] = []
  for (let block = from.block; block <= to.block; block++) {
    const surface = state.blocks[block]
    if (surface === undefined) continue
    const rows = rowCount(surface)
    const firstRow = block === from.block ? from.row : 0
    const lastRow = block === to.block ? to.row : rows - 1
    for (let row = firstRow; row <= lastRow; row++) {
      const startOffset = block === from.block && row === from.row ? from.offset : 0
      const endOffset =
        block === to.block && row === to.row ? to.offset : rowText(surface, row).length
      if (isText(surface)) {
        const prefix = startOffset === 0 ? markerOf(surface, row) : ''
        lines.push(prefix + rowText(surface, row).slice(startOffset, endOffset))
      } else if (startOffset === 0 && endOffset === rowText(surface, row).length) {
        lines.push(fullRowSource(surface, row))
      } else {
        lines.push(rowText(surface, row).slice(startOffset, endOffset))
      }
    }
  }
  return lines.join('\n')
}

export function deleteSelection(state: EditorState): EditorState {
  const [from, to] = unordered(state)
  if (compare(from, to) === 0) return state
  const surfaceFrom = state.blocks[from.block]
  const surfaceTo = state.blocks[to.block]
  if (surfaceFrom === undefined || surfaceTo === undefined) return state

  const blocks = state.blocks.map((block) => block)
  if (from.block === to.block) {
    const surface = blocks[from.block]!
    if (isText(surface)) {
      const startRow = surface.rows[from.row]
      const endRow = surface.rows[to.row]
      const rest = [...(startRow?.rest ?? []), ...(endRow?.rest ?? [])]
      const merged =
        startRow === undefined || endRow === undefined
          ? undefined
          : {
              ...startRow,
              text: startRow.text.slice(0, from.offset) + endRow.text.slice(to.offset),
              ...(rest.length > 0 ? { rest } : {}),
            }
      const rows = [
        ...surface.rows.slice(0, from.row),
        ...(merged === undefined ? [] : [merged]),
        ...surface.rows.slice(to.row + 1),
      ]
      blocks[from.block] = { ...surface, rows, edited: true }
      const next: EditorState = { ...state, blocks }
      return withCaret(next, from.block, from.row, from.offset)
    }
    const next: EditorState = { ...state, blocks: blocks.filter((_, index) => index !== from.block) }
    return withCaret(next, Math.max(0, from.block - 1), 0, 0)
  }

  const startText = isText(surfaceFrom) ? rowText(surfaceFrom, from.row).slice(0, from.offset) : ''
  const endText = isText(surfaceTo) ? rowText(surfaceTo, to.row).slice(to.offset) : ''
  const startRows = isText(surfaceFrom)
    ? [...surfaceFrom.rows.slice(0, from.row), { ...surfaceFrom.rows[from.row], text: startText }]
    : []
  const endRows = isText(surfaceTo)
    ? [{ ...surfaceTo.rows[to.row], text: endText }, ...surfaceTo.rows.slice(to.row + 1)]
    : []

  if (isText(surfaceFrom) && isText(surfaceTo)) {
    // A paragraph or heading holds one row, so the two outer pieces join into
    // one; a list, task or quote keeps its rows and takes the end block's rows
    // after the start block's.
    const singleRow = surfaceFrom.kind === 'paragraph' || surfaceFrom.kind === 'heading'
    const rows = singleRow
      ? [{ ...(surfaceFrom.rows[from.row] ?? { text: '' }), text: startText + endText }]
      : [...startRows, ...endRows]
    blocks[from.block] = { ...surfaceFrom, rows, edited: true }
    const next: EditorState = { ...state, blocks: blocks.filter((_, index) => index <= from.block || index > to.block) }
    return withCaret(next, from.block, from.row, from.offset)
  }

  if (isText(surfaceFrom) && surfaceFrom.rows.length > 0) {
    blocks[from.block] = { ...surfaceFrom, rows: startRows, edited: true }
  }
  const kept = blocks.filter((_, index) => index <= from.block || index > to.block)
  const next: EditorState = { ...state, blocks: kept }
  return withCaret(next, from.block, from.row, from.offset)
}

/* ---------------------------------------------------------- inline styles */

/**
 * Toggle one inline style over the selection inside one row. Cross-row and
 * cross-block selections are out of UI-19's pass (settled), so they are left
 * alone rather than guessed at.
 */
export function toggleStyle(state: EditorState, style: InlineStyle): EditorState {
  const [from, to] = unordered(state)
  if (from.block !== to.block || from.row !== to.row) return state
  const surface = state.blocks[from.block]
  if (surface === undefined || !isText(surface)) return state
  const text = rowText(surface, from.row)
  const result = toggleInlineStyle(text, { from: from.offset, to: to.offset }, style)
  if (result.source === text) return state
  const rows = surface.rows.map((row, index) =>
    index === from.row ? { ...row, text: result.source } : row,
  )
  const next = replaceBlockState(state, from.block, { ...surface, rows })
  const position = (offset: number): Position => ({ block: from.block, row: from.row, offset })
  return withSelection(next, position(result.selection.from), position(result.selection.to))
}

/**
 * Insert a link over the selection inside one row, the one creation path the
 * bar adds. The revealed markup is where the href is edited afterwards, so the
 * command writes the placeholder pair once and never toggles.
 */
export function insertLink(state: EditorState): EditorState {
  const [from, to] = unordered(state)
  if (from.block !== to.block || from.row !== to.row) return state
  const surface = state.blocks[from.block]
  if (surface === undefined || !isText(surface)) return state
  const text = rowText(surface, from.row)
  const result = linkInline(text, { from: from.offset, to: to.offset })
  if (result.source === text) return state
  const rows = surface.rows.map((row, index) =>
    index === from.row ? { ...row, text: result.source } : row,
  )
  const next = replaceBlockState(state, from.block, { ...surface, rows })
  const position = (offset: number): Position => ({ block: from.block, row: from.row, offset })
  return withSelection(next, position(result.selection.from), position(result.selection.to))
}

/* --------------------------------------------------------------- arrows */

function previousPosition(state: EditorState, position: Position): Position | null {
  if (position.row > 0) {
    const surface = state.blocks[position.block]
    if (surface === undefined) return null
    return { block: position.block, row: position.row - 1, offset: rowText(surface, position.row - 1).length }
  }
  if (position.block === 0) return null
  const previous = state.blocks[position.block - 1]
  if (previous === undefined) return null
  const row = rowCount(previous) - 1
  return { block: position.block - 1, row, offset: rowText(previous, row).length }
}

function nextPosition(state: EditorState, position: Position): Position | null {
  const surface = state.blocks[position.block]
  if (surface !== undefined && position.row < rowCount(surface) - 1) {
    return { block: position.block, row: position.row + 1, offset: 0 }
  }
  if (position.block >= state.blocks.length - 1) return null
  return { block: position.block + 1, row: 0, offset: 0 }
}

function applyMove(state: EditorState, position: Position | null, extend: boolean): EditorState {
  if (position === null) return state
  return extend
    ? withSelection(state, state.selection.anchor, position)
    : withSelection(state, position, position)
}

export function moveLeft(state: EditorState, extend: boolean): EditorState {
  const head = state.selection.head
  if (head.offset > 0) {
    return applyMove(state, { ...head, offset: head.offset - 1 }, extend)
  }
  return applyMove(state, previousPosition(state, head), extend)
}

export function moveRight(state: EditorState, extend: boolean): EditorState {
  const head = state.selection.head
  const surface = state.blocks[head.block]
  if (surface !== undefined && head.offset < rowText(surface, head.row).length) {
    return applyMove(state, { ...head, offset: head.offset + 1 }, extend)
  }
  return applyMove(state, nextPosition(state, head), extend)
}

function vertical(state: EditorState, delta: -1 | 1, extend: boolean): EditorState {
  const head = state.selection.head
  const surface = state.blocks[head.block]
  if (surface === undefined) return state
  if (delta < 0 && head.row > 0) {
    return applyMove(
      state,
      { block: head.block, row: head.row - 1, offset: Math.min(head.offset, rowText(surface, head.row - 1).length) },
      extend,
    )
  }
  if (delta > 0 && head.row < rowCount(surface) - 1) {
    return applyMove(
      state,
      { block: head.block, row: head.row + 1, offset: Math.min(head.offset, rowText(surface, head.row + 1).length) },
      extend,
    )
  }
  const target = delta < 0 ? previousPosition(state, { ...head, row: 0, offset: 0 }) : nextPosition(state, { ...head, row: rowCount(surface) - 1, offset: 0 })
  if (target === null) return state
  const targetSurface = state.blocks[target.block]
  if (targetSurface === undefined) return state
  return applyMove(
    state,
    { ...target, offset: Math.min(head.offset, rowText(targetSurface, target.row).length) },
    extend,
  )
}

export function moveUp(state: EditorState, extend: boolean): EditorState {
  return vertical(state, -1, extend)
}

export function moveDown(state: EditorState, extend: boolean): EditorState {
  return vertical(state, 1, extend)
}

/* ------------------------------------------------------------- shortcuts */

const SPACE_SHORTCUTS: Array<{
  pattern: RegExp
  build: (rest: string, match: RegExpExecArray) => SurfaceBlock | null
}> = [
  {
    pattern: /^#{1,3}\s$/,
    build: (rest, match) => ({
      kind: 'heading',
      level: (match[0] ?? '').trim().length,
      rows: [{ text: rest }],
      gap: '',
      source: '',
      edited: true,
      origin: { kind: 'paragraph', text: '', source: '', edited: true },
    }),
  },
  {
    pattern: /^- \[ \]\s$/,
    build: (rest) => ({ ...taskSurface(rest, false) }),
  },
  {
    pattern: /^- \[[xX]\]\s$/,
    build: (rest) => ({ ...taskSurface(rest, true) }),
  },
  {
    pattern: /^-\s$/,
    build: (rest) => ({ ...listSurface(rest, false, 1) }),
  },
  {
    pattern: /^(\d+)\.\s$/,
    build: (rest, match) => ({ ...listSurface(rest, true, Number.parseInt(match[1] ?? '1', 10)) }),
  },
  {
    pattern: /^>\s$/,
    build: (rest) => ({
      kind: 'quote',
      rows: [{ text: rest }],
      gap: '',
      source: '',
      edited: true,
      origin: { kind: 'paragraph', text: '', source: '', edited: true },
    }),
  },
]

function listSurface(text: string, ordered: boolean, start: number): SurfaceBlock {
  return {
    kind: 'list',
    ordered,
    start,
    rows: [{ text }],
    gap: '',
    source: '',
    edited: true,
    origin: { kind: 'paragraph', text: '', source: '', edited: true },
  }
}

function taskSurface(text: string, checked: boolean): SurfaceBlock {
  return {
    kind: 'task',
    rows: [{ text, checked }],
    gap: '',
    source: '',
    edited: true,
    origin: { kind: 'paragraph', text: '', source: '', edited: true },
  }
}

/**
 * Convert a paragraph as its shortcut token is completed. `space` fires on the
 * token's trailing space, `enter` on the fence and rule tokens; h4 to h6 never
 * convert.
 */
export function applyShortcut(state: EditorState, trigger: 'space' | 'enter'): EditorState {
  if (!collapsed(state)) return state
  const position = state.selection.head
  const surface = state.blocks[position.block]
  if (surface === undefined) return state

  if (surface.kind === 'list') {
    // `- ` has already converted the paragraph into a one-item bullet list, so
    // the longer task token has to win on that list row: `[ ] ` or `[x] ` at
    // the end of a one-item bullet list becomes a task.
    if (
      trigger !== 'space' ||
      surface.ordered === true ||
      surface.rows.length !== 1 ||
      position.row !== 0
    ) {
      return state
    }
    const text = rowText(surface, 0)
    if (position.offset !== text.length) return state
    const unchecked = /^\[ \]\s$/.exec(text)
    const checked = /^\[[xX]\]\s$/.exec(text)
    const match = unchecked ?? checked
    if (match === null) return state
    const rest = text.slice(match[0].length)
    const next: SurfaceBlock = {
      kind: 'task',
      rows: [{ text: rest, checked: checked !== null }],
      gap: surface.gap,
      source: '',
      edited: true,
      origin: surface.origin,
    }
    const blocks = state.blocks.map((block, index) => (index === position.block ? next : block))
    return withCaret({ ...state, blocks }, position.block, 0, rest.length)
  }

  if (surface.kind !== 'paragraph' || position.row !== 0) return state
  const text = rowText(surface, 0)
  if (position.offset !== text.length) return state

  let next: SurfaceBlock | null = null
  if (trigger === 'enter') {
    if (text === '```' || text === '~~~') {
      next = {
        kind: 'code',
        language: null,
        text: '',
        gap: surface.gap,
        source: '',
        edited: true,
        origin: surface.origin,
      }
    } else if (/^(?:-{3,}|\*{3,}|_{3,})$/.test(text)) {
      next = { kind: 'rule', gap: surface.gap, source: '', edited: true, origin: surface.origin }
    }
  } else {
    for (const shortcut of SPACE_SHORTCUTS) {
      const match = shortcut.pattern.exec(text)
      if (match === null) continue
      const rest = text.slice(match[0].length)
      const built = shortcut.build(rest, match as RegExpExecArray)
      if (built !== null) {
        next = { ...built, gap: surface.gap, origin: surface.origin }
      }
      break
    }
  }

  if (next === null) return state
  const blocks = state.blocks.map((block, index) => (index === position.block ? { ...next!, edited: true } : block))
  return withCaret({ ...state, blocks }, position.block, 0, next.kind === 'code' || next.kind === 'rule' ? 0 : restLength(next))
}

function restLength(surface: SurfaceBlock): number {
  return surface.kind === 'code' ? 0 : rowText(surface, 0).length
}

/* ------------------------------------------------------------- the commands */

/**
 * What a `/` command converts the block into: a UI-17 kind plus the subtype it
 * needs. The command table is the menu's only list of block types, and the
 * conversion reads the same table, so a kind the surface cannot author never
 * gets a row.
 */
export type CommandTarget = {
  kind: 'paragraph' | 'heading' | 'list' | 'task' | 'quote' | 'code' | 'rule'
  level?: 1 | 2 | 3
  ordered?: boolean
}

export type BlockCommand = {
  /** A stable identity for the row and its DOM id. */
  id: string
  label: string
  /** One line, shown beside the label. */
  description: string
  glyph: string
  /** The filter's other spellings: the markdown prefix and the common synonyms. */
  keywords: string[]
  target: CommandTarget
}

/**
 * The ten commands, in the reference's order. Presentation over UI-17's model:
 * Image, Page, Callout, Mermaid and Math have no row because they are not
 * editable block kinds (`math` is inline here), and `opaque` is what the model
 * does not cover, never a target a human picks.
 */
export const BLOCK_COMMANDS: readonly BlockCommand[] = [
  {
    id: 'text',
    label: 'Text',
    description: 'Plain text',
    glyph: '\u00b6',
    keywords: ['p', 'paragraph', 'plain'],
    target: { kind: 'paragraph' },
  },
  {
    id: 'heading-1',
    label: 'Heading 1',
    description: 'Top-level heading',
    glyph: 'H1',
    keywords: ['h1', 'heading 1', 'title', '#'],
    target: { kind: 'heading', level: 1 },
  },
  {
    id: 'heading-2',
    label: 'Heading 2',
    description: 'Section heading',
    glyph: 'H2',
    keywords: ['h2', 'heading 2', '##'],
    target: { kind: 'heading', level: 2 },
  },
  {
    id: 'heading-3',
    label: 'Heading 3',
    description: 'Subsection heading',
    glyph: 'H3',
    keywords: ['h3', 'heading 3', '###'],
    target: { kind: 'heading', level: 3 },
  },
  {
    id: 'bulleted',
    label: 'Bulleted list',
    description: 'An unordered list',
    glyph: '\u2022',
    keywords: ['ul', 'bullet', '-'],
    target: { kind: 'list', ordered: false },
  },
  {
    id: 'numbered',
    label: 'Numbered list',
    description: 'An ordered list',
    glyph: '1.',
    keywords: ['ol', 'numbered', '1.'],
    target: { kind: 'list', ordered: true },
  },
  {
    id: 'task',
    label: 'Task list',
    description: 'A checklist item',
    glyph: '\u2611',
    keywords: ['todo', 'task', 'checkbox', '[]'],
    target: { kind: 'task' },
  },
  {
    id: 'quote',
    label: 'Quote',
    description: 'Quoted text',
    glyph: '\u275d',
    keywords: ['quote', '>'],
    target: { kind: 'quote' },
  },
  {
    id: 'code',
    label: 'Code',
    description: 'A code block',
    glyph: '{}',
    keywords: ['code', 'fence', '```'],
    target: { kind: 'code' },
  },
  {
    id: 'divider',
    label: 'Divider',
    description: 'A horizontal rule',
    glyph: '\u2500',
    keywords: ['divider', 'rule', 'hr', '---'],
    target: { kind: 'rule' },
  },
]

/** The menu's rows for a query: a case-insensitive substring over label and keywords. */
export function filterCommands(query: string): BlockCommand[] {
  const needle = query.trim().toLowerCase()
  if (needle === '') return [...BLOCK_COMMANDS]
  return BLOCK_COMMANDS.filter(
    (command) =>
      command.label.toLowerCase().includes(needle) ||
      command.keywords.some((keyword) => keyword.toLowerCase().includes(needle)),
  )
}

function surfaceForTarget(target: CommandTarget, text: string, origin: SurfaceBlock): SurfaceBlock {
  const base: Base = { gap: origin.gap, source: '', edited: true, origin: origin.origin }
  switch (target.kind) {
    case 'paragraph':
      return { ...base, kind: 'paragraph', rows: [{ text }] }
    case 'heading':
      return { ...base, kind: 'heading', level: target.level ?? 1, rows: [{ text }] }
    case 'list':
      return {
        ...base,
        kind: 'list',
        ordered: target.ordered === true,
        start: 1,
        rows: [{ text }],
      }
    case 'task':
      return { ...base, kind: 'task', rows: [{ text, checked: false }] }
    case 'quote':
      return { ...base, kind: 'quote', rows: [{ text }] }
    case 'code':
      return { ...base, kind: 'code', language: null, text }
    case 'rule':
      return { ...base, kind: 'rule' }
  }
}

/**
 * Convert one single-row text block to the target kind: the one operation the
 * `/` menu and the toolbar's block rows share. Whatever the row held survives,
 * inline run source included, and so do the hidden children of a composite row
 * (they follow the block). A Divider holds no text, so the surviving text
 * follows it as a paragraph; the caret lands on the next block, or on a new
 * paragraph when the divider is last, so the note always ends somewhere to type.
 */
function convertSurface(
  state: EditorState,
  block: number,
  surface: TextSurface,
  target: CommandTarget,
  remaining: string,
  caret: number,
): EditorState {
  const converted = surfaceForTarget(target, remaining, surface)
  const children = surface.rows.flatMap((item) => item.rest ?? []).map((child) => surfaceOf(child, '\n'))
  const tail: SurfaceBlock[] =
    target.kind === 'rule' && remaining !== ''
      ? [surfaceForTarget({ kind: 'paragraph' }, remaining, surface)]
      : []
  const blocks = [
    ...state.blocks.slice(0, block),
    converted,
    ...tail,
    ...children,
    ...state.blocks.slice(block + 1),
  ]
  const next: EditorState = { ...state, blocks }
  if (target.kind === 'rule') {
    if (block + 1 < blocks.length) return withCaret(next, block + 1, 0, 0)
    const empty = surfaceForTarget({ kind: 'paragraph' }, '', surface)
    return withCaret({ ...next, blocks: [...blocks, { ...empty, gap: '\n\n' }] }, blocks.length, 0, 0)
  }
  return withCaret(next, block, 0, caret)
}

/**
 * Apply one `/` command to a text-bearing block.
 *
 * Only the leading `/query` goes: the `/` and the typed filter up to the caret.
 * The rest of the block survives through `convertSurface`.
 */
export function applyCommand(
  state: EditorState,
  block: number,
  command: BlockCommand,
): EditorState {
  const surface = state.blocks[block]
  if (surface === undefined || !isText(surface)) return state
  // A command converts the whole block, so it reaches only a block whose whole
  // content is the `/query`: one row. The second row of a list, a task list or
  // a quote has no place in the target kind, so converting would drop it.
  if (rowCount(surface) !== 1) return state

  const head = state.selection.head
  const row = head.block === block ? Math.min(head.row, rowCount(surface) - 1) : 0
  const text = rowText(surface, row)
  const cut = text.startsWith('/')
    ? head.block === block && head.row === row
      ? Math.max(1, Math.min(text.length, head.offset))
      : text.length
    : 0
  const remaining = text.slice(cut)
  const caret = cut > 0 ? 0 : Math.min(head.offset, remaining.length)
  return convertSurface(state, block, surface, command.target, remaining, caret)
}

/**
 * Convert the block the toolbar's block row names, keeping its text and inline
 * runs. The same `convertSurface` the `/` menu uses, so a bar click and a menu
 * pick are one operation.
 */
export function convertBlock(
  state: EditorState,
  block: number,
  target: CommandTarget,
): EditorState {
  const surface = state.blocks[block]
  if (surface === undefined || !isText(surface)) return state
  if (rowCount(surface) !== 1) return state
  const head = state.selection.head
  const text = rowText(surface, 0)
  const caret = head.block === block ? Math.min(head.offset, text.length) : 0
  return convertSurface(state, block, surface, target, text, caret)
}

/* -------------------------------------------------------------- structure */

/** A new empty paragraph after `block`, focused: the gutter's add-below. */
export function insertBelow(state: EditorState, block: number): EditorState {
  const surface = state.blocks[block]
  if (surface === undefined) return state
  const inserted: SurfaceBlock = {
    kind: 'paragraph',
    rows: [{ text: '' }],
    gap: '\n\n',
    source: '',
    edited: true,
    origin: { kind: 'paragraph', text: '', source: '', edited: false },
  }
  const blocks = [
    ...state.blocks.slice(0, block + 1),
    inserted,
    ...state.blocks.slice(block + 1),
  ]
  return withCaret({ ...state, blocks }, block + 1, 0, 0)
}

/**
 * Move one top-level block to a final index.
 *
 * The blocks are slots and their gaps belong to the slot, not to the block that
 * happens to sit in it: a move rewrites the order and leaves every separator
 * where it was, so each block's exact source travels untouched and the document
 * still parses back as the same blocks in the new order.
 */
export function moveBlock(state: EditorState, from: number, to: number): EditorState {
  const count = state.blocks.length
  if (from < 0 || from >= count) return state
  const target = Math.max(0, Math.min(count - 1, to))
  if (target === from) return state
  const moving = state.blocks[from]!
  const gaps = state.blocks.map((surface) => surface.gap)
  const reordered = state.blocks.filter((_, index) => index !== from)
  reordered.splice(target, 0, moving)
  const blocks = reordered.map((surface, index) => ({ ...surface, gap: gaps[index] ?? '' }))
  const shift = (position: Position): Position =>
    position.block === from ? { ...position, block: target } : position
  const next: EditorState = { ...state, blocks }
  return withSelection(next, shift(state.selection.anchor), shift(state.selection.head))
}

/**
 * The final index a drop lands on. A target block and the pointer's half name a
 * slot: the top half is before the target, the bottom half after it, and a drop
 * on the source itself is a no-op.
 */
export function dropIndex(from: number, target: number, below: boolean): number {
  if (from === target) return from
  if (from < target) return below ? target : target - 1
  return below ? target + 1 : target
}

/* ------------------------------------------------------------------ paste */

export function paste(state: EditorState, text: string): EditorState {
  const parsed = parseDocument(text)
  const pasted = fromDocument(parsed).blocks
  if (!text.includes('\n')) {
    return insertText(state, text)
  }

  let current = state
  if (!collapsed(current)) current = deleteSelection(current)
  const position = current.selection.head
  const surface = current.blocks[position.block]

  if (surface !== undefined && (surface.kind === 'paragraph' || surface.kind === 'heading')) {
    const source = rowText(surface, 0)
    const prefix = { ...surface, rows: [{ text: source.slice(0, position.offset) }], edited: true } as SurfaceBlock
    const suffix = { ...surface, rows: [{ text: source.slice(position.offset) }], edited: true } as SurfaceBlock
    const prefixEmpty = rowText(prefix, 0) === ''
    const suffixEmpty = rowText(suffix, 0) === ''
    const insert = pasted.map((block, index) => ({
      ...block,
      gap: index === 0 && prefixEmpty ? surface.gap : '\n',
    }))
    const blocks = [
      ...current.blocks.slice(0, position.block),
      ...(prefixEmpty ? [] : [prefix]),
      ...insert,
      ...(suffixEmpty ? [] : [{ ...suffix, gap: '\n' }]),
      ...current.blocks.slice(position.block + 1),
    ]
    const last = position.block + (prefixEmpty ? 0 : 1) + insert.length - 1
    const next: EditorState = { ...current, blocks }
    const lastSurface = next.blocks[last]
    const row = lastSurface === undefined ? 0 : rowCount(lastSurface) - 1
    return withCaret(next, Math.max(0, last), row, lastSurface === undefined ? 0 : rowText(lastSurface, row).length)
  }

  const blocks = [
    ...current.blocks.slice(0, position.block + 1),
    ...pasted.map((block) => ({ ...block, gap: '\n' })),
    ...current.blocks.slice(position.block + 1),
  ]
  const last = position.block + pasted.length
  const next: EditorState = { ...current, blocks }
  const lastSurface = next.blocks[last]
  const row = lastSurface === undefined ? 0 : rowCount(lastSurface) - 1
  return withCaret(next, Math.max(0, last), row, lastSurface === undefined ? 0 : rowText(lastSurface, row).length)
}

/* ------------------------------------------------------------------ tasks */

export function toggleTask(state: EditorState, block: number, row: number): EditorState {
  const surface = state.blocks[block]
  if (surface === undefined || surface.kind !== 'task') return state
  const rows = surface.rows.map((item, index) =>
    index === row ? { ...item, checked: item.checked !== true } : item,
  )
  return replaceBlockState(state, block, { ...surface, rows })
}

/** The text Mod+Shift+T files: the selection, or the caret's task line. */
export function candidateText(state: EditorState): string {
  const selected = selectedText(state)
  if (selected !== '') return selected
  const position = state.selection.head
  const surface = state.blocks[position.block]
  if (surface === undefined) return ''
  if (surface.kind === 'task' && surface.rows[position.row]?.checked === true) {
    return `- [x] ${rowText(surface, position.row)}`
  }
  if (surface.kind === 'task') return `- [ ] ${rowText(surface, position.row)}`
  return ''
}

/* ---------------------------------------------------------------- history */

/** Snapshot the state before an edit; a typing run joins the previous step. */
export function record(
  history: History,
  before: EditorState,
  options: { merge?: boolean } = {},
): History {
  if (options.merge === true && history.merging) return { ...history, future: [] }
  const past = [...history.past, before].slice(-HISTORY_LIMIT)
  return { past, future: [], merging: options.merge === true }
}

export function undo(history: History, current: EditorState): { state: EditorState; history: History } {
  const previous = history.past[history.past.length - 1]
  if (previous === undefined) return { state: current, history }
  return {
    state: previous,
    history: {
      past: history.past.slice(0, -1),
      future: [current, ...history.future],
      merging: false,
    },
  }
}

export function redo(history: History, current: EditorState): { state: EditorState; history: History } {
  const next = history.future[0]
  if (next === undefined) return { state: current, history }
  return {
    state: next,
    history: {
      past: [...history.past, current],
      future: history.future.slice(1),
      merging: false,
    },
  }
}

export type { Part }
