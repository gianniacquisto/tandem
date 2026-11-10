import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { BikeshedBlocks } from '../renderer/src/faces/BikeshedBlocks'
import { InlineRuns, WikilinkPicker } from '../renderer/src/faces/InlineRuns'
import { parseDocument } from '../renderer/src/shell/blocks'
import { parseInline, serialiseInline } from '../renderer/src/shell/inline'

const notes = ['Index.md', 'daily/Index.md', 'projects/fixture/fixture.md']

function draw(
  source: string,
  options: { notes?: string[]; selection?: { from: number; to: number } | null; editing?: boolean } = {},
): string {
  return renderToStaticMarkup(
    createElement(InlineRuns, {
      source,
      notes: options.notes ?? notes,
      selection: options.selection ?? null,
      editing: options.editing ?? true,
    }),
  )
}

/** The rendered row's text, as the Write surface reads it back. */
function textContent(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, '')
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
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

  it('draws inline math as its source text while the block is being edited', () => {
    const markup = draw('a $x^2$ b')
    expect(markup).toContain('run--math')
    expect(markup).toContain('$x^2$')
  })

  it('paints inline math as its raw delimiters until the chunk arrives', () => {
    // Static markup has no effects, which is the first-open gap UI-24 keeps:
    // the note paints with the source and the chunk fills the typeset math in.
    const markup = draw('a $x^2$ b', { editing: false })
    expect(markup).toContain('run--math')
    expect(markup).toContain('$x^2$')
    expect(markup).not.toContain('class="katex"')
    expect(markup).toContain('tandem-math-error')
  })

  it('renders the chip and run inside the Write surface', () => {
    const markup = renderToStaticMarkup(
      createElement(BikeshedBlocks, {
        document: parseDocument('**bold** and [[Index]] and $x^2$\n\n$$\nx = 1\n$$'),
        focusToken: 0,
        notePath: 'inline-runs.md',
        onChange: () => undefined,
        onSendToTodos: () => undefined,
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

describe('the drawing keeps the row source', () => {
  const sources = [
    'a **bold** b',
    'a _italic_ b',
    'a __bold__ b',
    'a ~~strike~~ b',
    'a `code` b',
    'a ``code`` b',
    'a _x_ and [[Index]] b',
    'Use snake_case and **bold** here',
    '**a [[Index]] b**',
    '*a [[Index]] b*',
    '**a $x$ b**',
    '[[Index|Alias]] and [Notes](folder/Notes.md) and $x$',
  ]

  it('draws the row so its textContent is exactly the source', () => {
    for (const source of sources) expect(textContent(draw(source)), source).toBe(source)
  })

  it('keeps a span open across a wikilink or math atom', () => {
    for (const source of ['**a [[Index]] b**', '*a [[Index]] b*', '**a $x$ b**']) {
      expect(serialiseInline(parseInline(source)), source).toBe(source)
    }

    const boldChip = draw('**a [[Index]] b**')
    expect(boldChip).toContain('<strong>')
    expect(boldChip).toContain('run--wikilink')
    expect(textContent(boldChip)).toBe('**a [[Index]] b**')

    const italicChip = draw('*a [[Index]] b*')
    expect(italicChip).toContain('<em>')
    expect(italicChip).toContain('run--wikilink')
    expect(textContent(italicChip)).toBe('*a [[Index]] b*')

    const boldMath = draw('**a $x$ b**')
    expect(boldMath).toContain('<strong>')
    expect(boldMath).toContain('run--math')
    expect(textContent(boldMath)).toBe('**a $x$ b**')
  })

  it('keeps the element for the three gate reproductions', () => {
    const italic = draw('a _italic_ b')
    expect(italic).toContain('<em>italic</em>')
    expect(textContent(italic)).toBe('a _italic_ b')

    const withChip = draw('a _x_ and [[Index]] b')
    expect(withChip).toContain('<em>x</em>')
    expect(withChip).toContain('run--wikilink')
    expect(textContent(withChip)).toBe('a _x_ and [[Index]] b')

    const intraword = draw('Use snake_case and **bold** here')
    expect(intraword).toContain('<strong>bold</strong>')
    expect(intraword).toContain('snake_case')
    expect(textContent(intraword)).toBe('Use snake_case and **bold** here')
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
