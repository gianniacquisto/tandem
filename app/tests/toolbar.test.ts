import { describe, expect, it } from 'vitest'

import { parseDocument } from '../renderer/src/shell/blocks'
import {
  fromDocument,
  serialise,
  withCaret,
  withSelection,
  type EditorState,
} from '../renderer/src/shell/editor-ops'
import { TOOLBAR_ROWS, toolbarStatus, type ToolbarAction } from '../renderer/src/shell/toolbar'

const surface = (source: string): EditorState => fromDocument(parseDocument(source))

const caret = (source: string, offset: number): EditorState =>
  withCaret(surface(source), 0, 0, offset)

const pressed = (state: EditorState, focused = true): Set<string> =>
  new Set(toolbarStatus(state, focused).pressed)

describe('the formatting toolbar state', () => {
  it("a run's style presses its toggle", () => {
    expect(pressed(caret('**bold**', 3)).has('bold')).toBe(true)
    expect(pressed(caret('plain', 2)).has('bold')).toBe(false)
    expect(pressed(caret('*italic*', 3)).has('italic')).toBe(true)
    expect(pressed(caret('~~strike~~', 3)).has('strikethrough')).toBe(true)
    expect(pressed(caret('`code`', 3)).has('inline-code')).toBe(true)
  })

  it('a partial selection is unpressed and a full one pressed', () => {
    const state = surface('**bo**ld')
    const partial = withSelection(
      state,
      { block: 0, row: 0, offset: 2 },
      { block: 0, row: 0, offset: 6 },
    )
    expect(pressed(partial).has('bold')).toBe(false)

    const full = withSelection(
      state,
      { block: 0, row: 0, offset: 2 },
      { block: 0, row: 0, offset: 4 },
    )
    expect(pressed(full).has('bold')).toBe(true)
  })

  it("a block row shows the focused block's kind", () => {
    const cases: Array<[string, string | null]> = [
      ['plain', null],
      ['# one', 'heading'],
      ['## two', 'heading'],
      ['### three', 'heading'],
      ['- a', 'bullet-list'],
      ['1. a', 'numbered-list'],
      ['- [ ] a', 'task-list'],
      ['> a', 'quote'],
      ['```\ncode\n```', 'code-block'],
    ]
    for (const [source, expected] of cases) {
      const pressedIds = pressed(caret(source, 0))
      for (const id of [
        'heading',
        'bullet-list',
        'numbered-list',
        'task-list',
        'quote',
        'code-block',
      ]) {
        expect(pressedIds.has(id), `${source} -> ${id}`).toBe(id === expected)
      }
    }
  })

  it('the heading row walks paragraph, H1, H2, H3 and back', () => {
    const heading = TOOLBAR_ROWS.find(
      (row): row is ToolbarAction => row.type === 'action' && row.id === 'heading',
    )
    expect(heading).toBeDefined()
    let state = surface('Title')
    const walked: string[] = []
    for (let press = 0; press < 4; press++) {
      state = heading!.command(state)
      walked.push(serialise(state))
    }
    expect(walked).toEqual(['# Title', '## Title', '### Title', 'Title'])
  })

  it('nothing editable means a disabled row', () => {
    expect(toolbarStatus(caret('| a | b |\n| - | - |', 0), true).disabled).toBe(true)
    expect(toolbarStatus(caret('plain', 0), false).disabled).toBe(true)
    expect(toolbarStatus(caret('plain', 0), true).disabled).toBe(false)
  })
})
