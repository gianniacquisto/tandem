import { describe, expect, it } from 'vitest'

import { parseDocument, serialiseDocument, type Block } from '../renderer/src/shell/blocks'
import {
  applyCommand,
  applyShortcut,
  backspace,
  BLOCK_COMMANDS,
  deleteForward,
  deleteSelection,
  dropIndex,
  emptyHistory,
  filterCommands,
  fromDocument,
  insertBelow,
  insertText,
  moveBlock,
  moveDown,
  moveLeft,
  moveRight,
  moveUp,
  paste,
  record,
  redo,
  rowText,
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

describe('an edit preserves the nested content it does not show', () => {
  it('keeps a nested list inside a list item', () => {
    const value = insertText(caret(state('- one\n  - two\n  - three'), 0, 0, 0), 'X')
    expect(serialise(value)).toBe('- Xone\n  - two\n  - three')
  })

  it('keeps a second paragraph in a list item', () => {
    const value = insertText(caret(state('- one\n\n  second para of the item'), 0, 0, 0), 'X')
    expect(serialise(value)).toBe('- Xone\n  second para of the item')
  })

  it('keeps a nested list inside a quote', () => {
    const value = insertText(caret(state('> one\n> - a\n> - b'), 0, 0, 3), 'X')
    expect(serialise(value)).toBe('> oneX\n> - a\n> - b')
  })

  it('keeps a nested list inside a task', () => {
    const value = insertText(caret(state('- [ ] one\n  - two'), 0, 0, 3), 'X')
    // The edited task block renders canonically: its marker is six columns, so
    // the nested list sits at its content column.
    expect(serialise(value)).toBe('- [ ] oneX\n      - two')
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

  it('converts a task marker typed one key at a time', () => {
    let value = caret(paragraphState(''), 0, 0, 0)
    for (const key of ['-', ' ', '[', ' ', ']', ' ']) {
      value = insertText(value, key)
      value = applyShortcut(value, 'space')
    }
    expect(value.blocks[0]).toMatchObject({ kind: 'task', rows: [{ text: '', checked: false }] })
    expect(serialise(value)).toBe('- [ ]')
  })

  it('converts a checked task marker typed one key at a time', () => {
    let value = caret(paragraphState(''), 0, 0, 0)
    for (const key of ['-', ' ', '[', 'x', ']', ' ']) {
      value = insertText(value, key)
      value = applyShortcut(value, 'space')
    }
    expect(value.blocks[0]).toMatchObject({ kind: 'task', rows: [{ text: '', checked: true }] })
    expect(serialise(value)).toBe('- [x]')
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

describe('the slash menu commands', () => {
  it("the command table is the model's editable kinds", () => {
    expect(BLOCK_COMMANDS.map((command) => command.label)).toEqual([
      'Text',
      'Heading 1',
      'Heading 2',
      'Heading 3',
      'Bulleted list',
      'Numbered list',
      'Task list',
      'Quote',
      'Code',
      'Divider',
    ])

    // Each column is unique: two rows a human cannot tell apart are a bug.
    for (const column of ['label', 'description', 'glyph'] as const) {
      const values = BLOCK_COMMANDS.map((command) => command[column])
      expect(new Set(values).size, column).toBe(values.length)
    }
    const keywords = BLOCK_COMMANDS.flatMap((command) => command.keywords)
    expect(new Set(keywords).size).toBe(keywords.length)

    // The targets are the model's editable kinds, in the reference's order.
    expect(BLOCK_COMMANDS.map((command) => command.target.kind)).toEqual([
      'paragraph',
      'heading',
      'heading',
      'heading',
      'list',
      'list',
      'task',
      'quote',
      'code',
      'rule',
    ])
    const headings = BLOCK_COMMANDS.filter((command) => command.target.kind === 'heading')
    expect(headings.map((command) => command.target.level)).toEqual([1, 2, 3])
    const lists = BLOCK_COMMANDS.filter((command) => command.target.kind === 'list')
    expect(lists.map((command) => command.target.ordered)).toEqual([false, true])

    // No command for a kind the surface cannot author.
    const kinds = new Set<string>(BLOCK_COMMANDS.map((command) => command.target.kind))
    for (const kind of ['image', 'math', 'opaque']) expect(kinds.has(kind), kind).toBe(false)

    // Every keyword the brief names is on the command it belongs to.
    const keywordOf = (label: string): string[] =>
      BLOCK_COMMANDS.find((command) => command.label === label)?.keywords ?? []
    expect(keywordOf('Text')).toEqual(['p', 'paragraph', 'plain'])
    expect(keywordOf('Heading 1')).toEqual(['h1', 'heading 1', 'title', '#'])
    expect(keywordOf('Heading 2')).toEqual(['h2', 'heading 2', '##'])
    expect(keywordOf('Heading 3')).toEqual(['h3', 'heading 3', '###'])
    expect(keywordOf('Bulleted list')).toEqual(['ul', 'bullet', '-'])
    expect(keywordOf('Numbered list')).toEqual(['ol', 'numbered', '1.'])
    expect(keywordOf('Task list')).toEqual(['todo', 'task', 'checkbox', '[]'])
    expect(keywordOf('Quote')).toEqual(['quote', '>'])
    expect(keywordOf('Code')).toEqual(['code', 'fence', '```'])
    expect(keywordOf('Divider')).toEqual(['divider', 'rule', 'hr', '---'])
  })

  it('the slash filter matches label and keywords', () => {
    const ids = (query: string): string[] =>
      filterCommands(query).map((command) => command.label)

    expect(ids('')).toHaveLength(10)
    expect(ids('   ')).toHaveLength(10)
    expect(ids('H2')).toEqual(['Heading 2'])
    expect(ids('heading 1')).toEqual(['Heading 1'])
    expect(ids('title')).toEqual(['Heading 1'])
    // A substring, as specified, not a token: `#` is inside `##` and `###`.
    expect(ids('#')).toEqual(['Heading 1', 'Heading 2', 'Heading 3'])
    // A substring, not a fuzzy score: `##` is inside `###` too.
    expect(ids('##')).toEqual(['Heading 2', 'Heading 3'])
    // A substring, not a fuzzy score: `ul` sits inside `rule` too.
    expect(ids('ul')).toEqual(['Bulleted list', 'Divider'])
    expect(ids('bullet')).toEqual(['Bulleted list'])
    expect(ids('-')).toEqual(['Bulleted list', 'Divider'])
    expect(ids('1.')).toEqual(['Numbered list'])
    expect(ids('todo')).toEqual(['Task list'])
    expect(ids('[]')).toEqual(['Task list'])
    expect(ids('>')).toEqual(['Quote'])
    expect(ids('```')).toEqual(['Code'])
    expect(ids('---')).toEqual(['Divider'])
    expect(ids('no such block')).toEqual([])
  })

  it("a type change keeps the block's text and runs", () => {
    // The caret sits after the typed query, so the run source after it survives.
    const start = caret(state('/quo **bold** and plain'), 0, 0, 5)
    const quote = BLOCK_COMMANDS.find((command) => command.label === 'Quote')!
    const changed = applyCommand(start, 0, quote)

    expect(kinds(changed)).toEqual(['quote'])
    expect(rowText(changed.blocks[0]!, 0)).toBe('**bold** and plain')
    expect(changed.selection.head).toEqual({ block: 0, row: 0, offset: 0 })
    expect(serialise(changed)).toBe('> **bold** and plain')
    // The converted block is canonical; the old source does not round-trip through it.
    expect(changed.blocks[0]!.edited).toBe(true)

    const heading = BLOCK_COMMANDS.find((command) => command.label === 'Heading 2')!
    const titled = applyCommand(caret(state('/h2 Title'), 0, 0, 4), 0, heading)
    expect(titled.blocks[0]!.kind).toBe('heading')
    expect(rowText(titled.blocks[0]!, 0)).toBe('Title')
    expect(serialise(titled)).toBe('## Title')

    const list = applyCommand(caret(state('/1. one'), 0, 0, 4), 0,
      BLOCK_COMMANDS.find((command) => command.label === 'Numbered list')!)
    expect(serialise(list)).toBe('1. one')

    const task = applyCommand(caret(state('/todo call'), 0, 0, 6), 0,
      BLOCK_COMMANDS.find((command) => command.label === 'Task list')!)
    expect(serialise(task)).toBe('- [ ] call')

    const code = applyCommand(caret(state('/code x = 1'), 0, 0, 6), 0,
      BLOCK_COMMANDS.find((command) => command.label === 'Code')!)
    expect(code.blocks[0]!.kind).toBe('code')
    expect(serialise(code)).toBe('```\nx = 1\n```')
  })

  it('a divider conversion leaves somewhere to type', () => {
    const divider = BLOCK_COMMANDS.find((command) => command.label === 'Divider')!
    const last = applyCommand(caret(state('/hr'), 0, 0, 3), 0, divider)
    expect(kinds(last)).toEqual(['rule', 'paragraph'])
    expect(last.selection.head).toEqual({ block: 1, row: 0, offset: 0 })
    expect(serialise(last)).toBe('---\n\n')

    // With a block below, the divider takes the focus there instead of appending.
    const next = applyCommand(caret(state('/hr\n\ntext'), 0, 0, 3), 0, divider)
    expect(kinds(next)).toEqual(['rule', 'paragraph'])
    expect(next.selection.head).toEqual({ block: 1, row: 0, offset: 0 })
    expect(serialise(next)).toBe('---\n\ntext')
  })

  it('a two-item list refuses a type change and keeps its content', () => {
    const list = state('- /\n- b')
    expect(serialise(list)).toBe('- /\n- b')

    const text = BLOCK_COMMANDS.find((command) => command.label === 'Text')!
    expect(applyCommand(list, 0, text)).toBe(list)
    expect(serialise(list)).toBe('- /\n- b')
  })

  it('a two-line quote refuses a type change and keeps its content', () => {
    const quote = state('> /\n> b')
    expect(serialise(quote)).toBe('> /\n> b')

    const heading = BLOCK_COMMANDS.find((command) => command.label === 'Heading 1')!
    expect(applyCommand(quote, 0, heading)).toBe(quote)
    expect(serialise(quote)).toBe('> /\n> b')
  })

  it('the add-below inserts a paragraph after the block', () => {
    const middle = insertBelow(state('one\n\ntwo'), 0)
    expect(kinds(middle)).toEqual(['paragraph', 'paragraph', 'paragraph'])
    expect(rowText(middle.blocks[1]!, 0)).toBe('')
    expect(middle.selection.head).toEqual({ block: 1, row: 0, offset: 0 })
    expect(serialise(middle)).toBe('one\n\n\n\ntwo')

    // The empty note is already a paragraph, and the add-below still appends one.
    const empty = state('')
    expect(empty.blocks).toHaveLength(1)
    const after = insertBelow(empty, 0)
    expect(kinds(after)).toEqual(['paragraph', 'paragraph'])
    expect(after.selection.head).toEqual({ block: 1, row: 0, offset: 0 })

    // A list row is one top-level block: the plus adds a paragraph after the list.
    const list = insertBelow(state('- a\n- b'), 0)
    expect(kinds(list)).toEqual(['list', 'paragraph'])
    expect(serialise(list)).toBe('- a\n- b\n\n')

    expect(insertBelow(state('one'), 3)).toEqual(state('one'))
  })

  it("a move preserves every block's bytes", () => {
    const start = state('a\n\nb\n\nc')
    expect(start.blocks.map((block) => block.source)).toEqual(['a', 'b', 'c'])

    const down = moveBlock(start, 0, 2)
    expect(serialise(down)).toBe('b\n\nc\n\na')
    expect(down.blocks.map((block) => block.source)).toEqual(['b', 'c', 'a'])
    expect(down.blocks.map((block) => block.edited)).toEqual([false, false, false])
    expect(parseDocument(serialise(down)).parts.filter((part) => part.type === 'block')).toHaveLength(3)

    const up = moveBlock(start, 2, 0)
    expect(serialise(up)).toBe('c\n\na\n\nb')
    expect(up.blocks.map((block) => block.source)).toEqual(['c', 'a', 'b'])

    expect(moveBlock(start, 1, 1)).toBe(start)
    expect(moveBlock(start, 0, 0)).toBe(start)
    expect(serialise(moveBlock(start, 2, 9))).toBe('a\n\nb\n\nc')
    expect(serialise(moveBlock(start, 0, -1))).toBe('a\n\nb\n\nc')

    // The caret rides the moved block, so the keyboard move keeps typing there.
    const moved = moveBlock(caret(state('one\n\ntwo\n\nthree'), 0, 0, 2), 0, 1)
    expect(serialise(moved)).toBe('two\n\none\n\nthree')
    expect(moved.selection.head).toEqual({ block: 1, row: 0, offset: 2 })
  })

  it('the drop maps a target block and a half onto a final index', () => {
    // Dropping on the top half lands before the target, the bottom half after it.
    expect(dropIndex(0, 1, false)).toBe(0)
    expect(dropIndex(0, 1, true)).toBe(1)
    expect(dropIndex(2, 1, false)).toBe(1)
    expect(dropIndex(2, 1, true)).toBe(2)
    expect(dropIndex(0, 2, false)).toBe(1)
    expect(dropIndex(0, 2, true)).toBe(2)
    // A drop on the source is a no-op, whichever half.
    expect(dropIndex(1, 1, false)).toBe(1)
    expect(dropIndex(1, 1, true)).toBe(1)
  })
})
