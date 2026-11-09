import { describe, expect, it } from 'vitest'

import { parseDocument, serialiseDocument, type Block } from '../renderer/src/shell/blocks'
import {
  applyShortcut,
  backspace,
  deleteForward,
  deleteSelection,
  emptyHistory,
  fromDocument,
  insertText,
  moveDown,
  moveLeft,
  moveRight,
  moveUp,
  paste,
  record,
  redo,
  selectAll,
  selectedText,
  serialise,
  split,
  toggleTask,
  undo,
  withCaret,
  withSelection,
  type EditorState,
  type History,
} from '../renderer/src/shell/editor-ops'

const state = (text: string): EditorState => fromDocument(parseDocument(text))

/** A paragraph whose text is the raw token, before it is serialised or reparsed. */
const paragraphState = (text: string): EditorState =>
  fromDocument({
    parts: [{ type: 'block', block: { kind: 'paragraph', text, source: '', edited: true } }],
  })

const caret = (value: EditorState, block: number, row: number, offset: number): EditorState =>
  withCaret(value, block, row, offset)

function kinds(value: EditorState): string[] {
  return value.blocks.map((block) => block.kind)
}

function edit(
  start: EditorState,
  change: (value: EditorState) => EditorState,
): EditorState {
  return change(start)
}

describe('Enter splits a text block at the caret', () => {
  it('splits a paragraph at start, middle and end', () => {
    expect(serialise(edit(caret(state('hello world'), 0, 0, 0), split))).toBe('\nhello world')
    expect(serialise(edit(caret(state('hello world'), 0, 0, 5), split))).toBe('hello\n world')
    expect(serialise(edit(caret(state('hello world'), 0, 0, 11), split))).toBe('hello world\n')
  })

  it('a heading yields the heading and a paragraph below', () => {
    const value = edit(caret(state('## Title'), 0, 0, 2), split)
    expect(kinds(value)).toEqual(['heading', 'paragraph'])
    expect(serialise(value)).toBe('## Ti\ntle')
  })

  it('a list item yields the next item', () => {
    const value = edit(caret(state('- one\n- two'), 0, 0, 3), split)
    expect(serialise(value)).toBe('- one\n-\n- two')
    expect(value.selection.head).toEqual({ block: 0, row: 1, offset: 0 })
  })

  it('an empty item ends the list', () => {
    const value = edit(caret(state('- one'), 0, 0, 3), split)
    const ended = edit(caret(value, 0, 1, 0), split)
    expect(serialise(ended)).toBe('- one')
    expect(ended.blocks).toHaveLength(1)
    expect(ended.selection.head).toEqual({ block: 0, row: 0, offset: 3 })
  })

  it('a quote line yields the next line', () => {
    expect(serialise(edit(caret(state('> one'), 0, 0, 3), split))).toBe('> one\n>')
  })

  it('a code block takes a newline', () => {
    expect(serialise(edit(caret(state('```ts\nx\n```'), 0, 0, 1), split))).toBe('```ts\nx\n\n```')
  })

  it('a rule and an image are inert', () => {
    expect(serialise(edit(caret(state('---'), 0, 0, 0), split))).toBe('---')
    expect(serialise(edit(caret(state('![a](b.png)'), 0, 0, 0), split))).toBe('![a](b.png)')
  })

  it('a paragraph with a hard line break stays one block', () => {
    const value = insertText(caret(state('one  \ntwo'), 0, 0, 3), 'X')
    expect(value.blocks).toHaveLength(1)
    expect(serialise(value)).toBe('oneX  \ntwo')
  })
})

describe('Backspace merges and Delete mirrors', () => {
  it('merges a paragraph into the paragraph above', () => {
    expect(serialise(edit(caret(state('one\n\ntwo'), 1, 0, 0), backspace))).toBe('onetwo')
    expect(serialise(edit(caret(state('one\n\ntwo'), 0, 0, 3), deleteForward))).toBe('onetwo')
  })

  it('converts an incompatible block to a paragraph after the previous one', () => {
    expect(serialise(edit(caret(state('one\n\n# two'), 1, 0, 0), backspace))).toBe('one\n\ntwo')
    expect(serialise(edit(caret(state('one\n\n- two'), 1, 0, 0), backspace))).toBe('one\n\ntwo')
  })

  it('removes an empty block', () => {
    const splitState = edit(caret(state('a'), 0, 0, 1), split)
    expect(serialise(splitState)).toBe('a\n')
    expect(serialise(edit(caret(splitState, 1, 0, 0), backspace))).toBe('a')
  })

  it('deletes a rule and an image', () => {
    expect(serialise(edit(caret(state('---'), 0, 0, 0), backspace))).toBe('')
    expect(serialise(edit(caret(state('![a](b.png)'), 0, 0, 0), deleteForward))).toBe('')
  })

  it('merges a list row into the row above', () => {
    expect(serialise(edit(caret(state('- one\n- two'), 0, 1, 0), backspace))).toBe('- onetwo')
  })
})

describe('the markdown shortcuts', () => {
  const shortcut = (text: string, trigger: 'space' | 'enter' = 'space'): EditorState =>
    applyShortcut(caret(paragraphState(text), 0, 0, text.length), trigger)

  it('converts a heading token on the space', () => {
    const one = shortcut('# ')
    expect(one.blocks[0]).toMatchObject({ kind: 'heading', level: 1, rows: [{ text: '' }] })
    expect(serialise(one)).toBe('# ')
    expect(shortcut('## ').blocks[0]).toMatchObject({ kind: 'heading', level: 2 })
    expect(shortcut('### ').blocks[0]).toMatchObject({ kind: 'heading', level: 3 })
  })

  it('does not convert h4 to h6, and escapes the paragraph instead', () => {
    expect(shortcut('#### ').blocks[0]?.kind).toBe('paragraph')
    expect(serialise(shortcut('#### '))).toBe('\\#### ')
    expect(shortcut('##### ').blocks[0]?.kind).toBe('paragraph')
  })

  it('converts a bullet, an ordered list and a task', () => {
    expect(serialise(shortcut('- '))).toBe('-')
    expect(shortcut('1. ').blocks[0]).toMatchObject({ kind: 'list', ordered: true, start: 1 })
    expect(shortcut('3. ').blocks[0]).toMatchObject({ kind: 'list', ordered: true, start: 3 })
    const task = shortcut('- [ ] ')
    expect(task.blocks[0]).toMatchObject({ kind: 'task', rows: [{ text: '', checked: false }] })
    expect(shortcut('- [x] ').blocks[0]).toMatchObject({ kind: 'task', rows: [{ checked: true }] })
  })

  it('converts a quote', () => {
    expect(shortcut('> ').blocks[0]).toMatchObject({ kind: 'quote', rows: [{ text: '' }] })
  })

  it('converts a fence and a rule on Enter', () => {
    expect(serialise(shortcut('```', 'enter'))).toBe('```\n```')
    expect(shortcut('```', 'enter').blocks[0]).toMatchObject({ kind: 'code' })
    expect(serialise(shortcut('---', 'enter'))).toBe('---')
    expect(shortcut('---', 'enter').blocks[0]?.kind).toBe('rule')
  })

  it('does not fire a space shortcut on Enter', () => {
    expect(shortcut('# ', 'enter').blocks[0]?.kind).toBe('paragraph')
  })
})

describe('paste, the task toggle and the escape', () => {
  it('pastes multiple markdown lines as blocks', () => {
    const value = paste(caret(state(''), 0, 0, 0), '- a\n- b')
    expect(value.blocks.map((block) => block.kind)).toEqual(['list'])
    expect(serialise(value)).toBe('- a\n- b')
  })

  it('pastes a table as a read-only opaque block', () => {
    const value = paste(caret(state(''), 0, 0, 0), '| a | b |\n| - | - |')
    expect(value.blocks[0]?.kind).toBe('opaque')
    expect(serialise(value)).toBe('| a | b |\n| - | - |')
  })

  it('pastes a single line inline', () => {
    const value = paste(caret(state('ab'), 0, 0, 1), 'X')
    expect(value.blocks).toHaveLength(1)
    expect(serialise(value)).toBe('aXb')
  })

  it('toggles a task item', () => {
    const value = state('- [ ] one\n- [x] two')
    expect(serialise(toggleTask(value, 0, 0))).toBe('- [x] one\n- [x] two')
  })

  it('escapes a paragraph that starts with a marker so it reopens as a paragraph', () => {
    const paragraph: Block = {
      kind: 'paragraph',
      text: '# not a heading',
      source: '',
      edited: true,
    }
    const value = fromDocument({ parts: [{ type: 'block', block: paragraph }] })
    const text = serialise(value)
    expect(text).toBe('\\# not a heading')
    const reopened = parseDocument(text)
    expect(reopened.parts[0]).toMatchObject({ type: 'block', block: { kind: 'paragraph' } })
  })
})

describe('arrows and selection cross blocks', () => {
  it('moves left and right across block boundaries', () => {
    const value = caret(state('ab\n\ncd'), 0, 0, 2)
    const right = moveRight(value, false)
    expect(right.selection.head).toEqual({ block: 1, row: 0, offset: 0 })
    expect(moveLeft(right, false).selection.head).toEqual({ block: 0, row: 0, offset: 2 })
  })

  it('moves up and down between blocks at the same column', () => {
    const value = caret(state('abc\n\ndefg'), 0, 0, 2)
    const down = moveDown(value, false)
    expect(down.selection.head).toEqual({ block: 1, row: 0, offset: 2 })
    expect(moveUp(down, false).selection.head).toEqual({ block: 0, row: 0, offset: 2 })
  })

  it('moves across list rows', () => {
    const value = caret(state('- one\n- two'), 0, 0, 3)
    expect(moveRight(value, false).selection.head).toEqual({ block: 0, row: 1, offset: 0 })
  })

  it('selects the whole note and deletes the selection', () => {
    const value = selectAll(state('ab\n\ncd'))
    expect(value.selection.anchor).toEqual({ block: 0, row: 0, offset: 0 })
    expect(value.selection.head).toEqual({ block: 1, row: 0, offset: 2 })
    expect(serialise(deleteSelection(value))).toBe('')
  })

  it('copies the selected source across blocks', () => {
    const value = withSelection(
      state('ab\n\ncd'),
      { block: 0, row: 0, offset: 1 },
      { block: 1, row: 0, offset: 1 },
    )
    expect(selectedText(value)).toBe('b\nc')
  })

  it('deletes across blocks by joining the two outer pieces', () => {
    const value = withSelection(
      state('ab\n\ncd'),
      { block: 0, row: 0, offset: 1 },
      { block: 1, row: 0, offset: 1 },
    )
    expect(serialise(deleteSelection(value))).toBe('ad')
  })
})

describe('undo and redo', () => {
  it('merges a typing run into one step', () => {
    const start = caret(state('a'), 0, 0, 1)
    let history: History = emptyHistory
    const b = insertText(start, 'b')
    history = record(history, start, { merge: true })
    const bc = insertText(b, 'c')
    history = record(history, b, { merge: true })

    const undone = undo(history, bc)
    expect(history.past).toHaveLength(1)
    expect(serialise(undone.state)).toBe('a')
    const redone = redo(undone.history, undone.state)
    expect(serialise(redone.state)).toBe('abc')
  })

  it('does not merge across a structural edit', () => {
    const start = caret(state('ab'), 0, 0, 2)
    let history: History = emptyHistory
    const typed = insertText(start, 'c')
    history = record(history, start, { merge: true })
    const splitState = split(typed)
    history = record(history, typed, { merge: false })
    expect(history.past).toHaveLength(2)
    expect(serialise(undo(history, splitState).state)).toBe('abc')
  })
})
