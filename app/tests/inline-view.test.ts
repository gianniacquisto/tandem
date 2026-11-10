import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { BikeshedBlocks } from '../renderer/src/faces/BikeshedBlocks'
import { InlineRuns, WikilinkPicker } from '../renderer/src/faces/InlineRuns'
import { parseDocument } from '../renderer/src/shell/blocks'

const notes = ['Index.md', 'daily/Index.md', 'projects/fixture/fixture.md']

function draw(
  source: string,
  options: { notes?: string[]; selection?: { from: number; to: number } | null } = {},
): string {
  return renderToStaticMarkup(
    createElement(InlineRuns, {
      source,
      notes: options.notes ?? notes,
      selection: options.selection ?? null,
    }),
  )
}

describe('the inline runs draw their elements', () => {
  it('draws each text style as its own element', () => {
    const markup = draw('**bold** *italic* ~~strike~~ `code`')
    expect(markup).toContain('<strong>')
    expect(markup).toContain('<em>')
    expect(markup).toContain('<del>')
    expect(markup).toContain('<code>')
    expect(markup).toContain('bold')
    expect(markup).toContain('italic')
    expect(markup).toContain('strike')
    expect(markup).toContain('code')
  })

  it('draws a markdown link run with its href as the title', () => {
    const markup = draw('[Notes](folder/Notes.md)')
    expect(markup).toContain('run--link')
    expect(markup).toContain('title="folder/Notes.md"')
    expect(markup).toContain('Notes')
  })

  it('draws a wikilink as a chip with its display text', () => {
    const markup = draw('[[daily/Index|Today]]')
    expect(markup).toContain('run--wikilink')
    expect(markup).toContain('wikilink-chip')
    expect(markup).toMatch(/wikilink-chip[^>]*>Today</)
    expect(markup).not.toContain('run--wikilink-broken')
  })

  it('draws a broken chip with its broken class and a title naming the target', () => {
    const markup = draw('[[Ghost note]]', { notes: ['Index.md'] })
    expect(markup).toContain('run--wikilink-broken')
    expect(markup).toContain('title="Missing note: Ghost note"')
  })

  it('draws inline math as its source text in the accent ink', () => {
    const markup = draw('a $x^2$ b')
    expect(markup).toContain('run--math')
    expect(markup).toContain('$x^2$')
  })

  it('renders the chip and run inside the Write surface', () => {
    const markup = renderToStaticMarkup(
      createElement(BikeshedBlocks, {
        document: parseDocument('**bold** and [[Index]] and $x^2$\n\n$$\nx = 1\n$$'),
        focusToken: 0,
        fontSize: 16,
        notePath: 'inline-runs.md',
        onChange: () => undefined,
        onSendToTodos: () => undefined,
        onSwitchToSource: () => undefined,
        noteIndex: notes,
      }),
    )
    expect(markup).toContain('<strong>')
    expect(markup).toContain('run--wikilink')
    expect(markup).toContain('run--math')
    expect(markup).toContain('block--math')
    expect(markup).toContain('x = 1')
  })
})

describe('the reveal state', () => {
  it('shows the markers when the caret lies inside a run', () => {
    const inside = draw('a **bold** b', { selection: { from: 4, to: 4 } })
    expect(inside).toContain('run__marker--shown')
    expect(inside).toContain('**')
  })

  it('keeps the markers hidden when the caret is outside', () => {
    const outside = draw('a **bold** b', { selection: { from: 0, to: 0 } })
    expect(outside).not.toContain('run__marker--shown')
  })

  it('reveals for any part of a selection inside the run', () => {
    const crossed = draw('a **bold** b', { selection: { from: 0, to: 5 } })
    expect(crossed).toContain('run__marker--shown')
  })
})

describe('the wikilink picker', () => {
  it('draws a row per matching note with the active row marked', () => {
    const markup = renderToStaticMarkup(
      createElement(WikilinkPicker, { notes, query: 'ind', active: 1 }),
    )
    expect(markup.match(/role="option"/g)).toHaveLength(2)
    expect(markup).toContain('aria-selected="true"')
    expect(markup).toContain('Index')
    expect(markup).toContain('daily/Index')
  })

  it('draws a two-word empty state and no rows when nothing matches', () => {
    const markup = renderToStaticMarkup(
      createElement(WikilinkPicker, { notes, query: 'zzz', active: 0 }),
    )
    expect(markup).toContain('No notes')
    expect(markup).not.toContain('role="option"')
  })
})
