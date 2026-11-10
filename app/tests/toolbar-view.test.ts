import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { parseDocument } from '../renderer/src/shell/blocks'
import { fromDocument, withCaret, type EditorState } from '../renderer/src/shell/editor-ops'
import { BikeshedToolbar } from '../renderer/src/faces/BikeshedToolbar'

const LABELS = [
  'Bold',
  'Italic',
  'Strikethrough',
  'Inline code',
  'Link',
  'Heading',
  'Bullet list',
  'Numbered list',
  'Task list',
  'Quote',
  'Code block',
]

function draw(state: EditorState, focused = true): string {
  return renderToStaticMarkup(
    createElement(BikeshedToolbar, { state, focused, onRun: () => undefined }),
  )
}

const plain = (): EditorState => fromDocument(parseDocument('plain'))

function labelsIn(markup: string): string[] {
  return [...markup.matchAll(/<button[^>]*aria-label="([^"]+)"/g)].map(
    (match) => match[1] as string,
  )
}

describe('the toolbar row', () => {
  it('the row draws eleven actions in order, split by one separator', () => {
    const markup = draw(plain())
    expect((markup.match(/<button/g) ?? []).length).toBe(LABELS.length)
    expect(LABELS).toHaveLength(11)
    expect(labelsIn(markup)).toEqual(LABELS)
    expect((markup.match(/bikeshed__toolbar-sep/g) ?? []).length).toBe(1)
    expect(markup).toContain('Heading (cycles H1 \u2192 H2 \u2192 H3 \u2192 body)')
    expect(markup).toContain('<svg')
  })

  it('the pressed and disabled states reach the markup', () => {
    const bold = draw(withCaret(fromDocument(parseDocument('**bold**')), 0, 0, 3))
    expect(bold).toMatch(/aria-label="Bold"[^>]*aria-pressed="true"/)
    expect((bold.match(/aria-pressed="true"/g) ?? []).length).toBe(1)
    expect((bold.match(/aria-pressed="false"/g) ?? []).length).toBe(10)
    expect(bold).not.toContain('disabled')

    const inert = draw(plain(), false)
    expect((inert.match(/disabled=""/g) ?? []).length).toBe(11)
    // A disabled button keeps its title.
    expect(inert).toContain('title="Bold"')
  })

  it('no font-size control and no Divider or Text row', () => {
    const markup = draw(plain())
    expect(markup.toLowerCase()).not.toContain('font')
    expect(labelsIn(markup)).not.toContain('Divider')
    expect(labelsIn(markup)).not.toContain('Text')
  })
})
