import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { MarkdownDocument } from '../renderer/src/faces/MarkdownDocument'
import type { MarkdownBlockView } from '../renderer/src/wire/MarkdownBlockView'
import type { MarkdownInlineView } from '../renderer/src/wire/MarkdownInlineView'
import { token } from '../shots/tokens'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
/** A Windows checkout reads the same stylesheet with CRLF endings. */
const css = readFileSync(`${appRoot}renderer/src/app.css`, 'utf8').replace(/\r\n/g, '\n')

function paragraph(run: MarkdownInlineView): MarkdownBlockView[] {
  return [{ kind: 'paragraph', runs: [run] }]
}

/** The drawing as the browser would receive it, without a window. */
function draw(blocks: MarkdownBlockView[]): string {
  return renderToStaticMarkup(createElement(MarkdownDocument, { blocks }))
}

describe('the markdown drawing', () => {
  it('draws a run with one style as before', () => {
    expect(draw(paragraph({ text: 'bold', styles: ['bold'], link: null }))).toBe(
      '<div class="markdown"><p class="markdown__paragraph"><strong>bold</strong></p></div>',
    )
    expect(draw(paragraph({ text: 'code', styles: ['code'], link: null }))).toBe(
      '<div class="markdown"><p class="markdown__paragraph"><code class="markdown__inline-code">code</code></p></div>',
    )
  })

  it('composes every style a run carries, not only the first', () => {
    // `**bold with `code`**` is one run carrying both: the domain keeps a set.
    expect(
      draw(paragraph({ text: 'both', styles: ['bold', 'code'], link: null })),
    ).toBe(
      '<div class="markdown"><p class="markdown__paragraph"><code class="markdown__inline-code"><strong>both</strong></code></p></div>',
    )
    expect(
      draw(paragraph({ text: 'all', styles: ['bold', 'italic', 'strikethrough'], link: null })),
    ).toBe(
      '<div class="markdown"><p class="markdown__paragraph"><strong><em><s>all</s></em></strong></p></div>',
    )
  })

  it('keeps the link span inside the styles it wears', () => {
    expect(
      draw(paragraph({ text: 'link', styles: ['bold'], link: 'https://example.com' })),
    ).toBe(
      '<div class="markdown"><p class="markdown__paragraph"><strong><span class="markdown__link" title="https://example.com">link</span></strong></p></div>',
    )
  })

  it('draws a link label in the sheet\'s warm ink, as the Swift label did', () => {
    // `Sources/Tandem/MarkdownDocumentView.swift:145` was `palette.emberDeep`.
    const block = css.match(/\.markdown__link\s*\{([^}]*)\}/)?.[1] ?? ''
    const colour = block.match(/color:\s*var\(--([\w-]+)\)/)?.[1]
    if (colour === undefined) throw new Error(`.markdown__link draws no token colour: ${block}`)

    // The name is not the colour: a token the sheet does not define leaves the
    // label on the inherited ink. Resolve it through the day palette, the way
    // `shots/tokens.ts` reads the photographed faces, and compare it to the
    // Swift label's emberDeep, which is what a link is meant to wear.
    expect(token(colour)).toBe(token('ember-deep'))
  })
})

describe('the markdown drawing under payload drift (TAN-312)', () => {
  /** A block the wire never emitted: a future core kind, or a hand-made reply. */
  function unknownKind(): MarkdownBlockView[] {
    return [{ kind: 'alert', text: 'heads up' } as unknown as MarkdownBlockView]
  }

  it('draws an unknown block kind as a fallback, never nothing (and never a throw)', () => {
    // A component that returns undefined unmounts the whole root on the
    // client: the sheet must draw something instead.
    expect(draw(unknownKind())).toContain('cannot draw')
  })

  it('draws the fallback inline, inside the list item that carried it', () => {
    const nested = [
      {
        kind: 'list',
        ordered: false,
        start: 1,
        items: [[{ kind: 'alert', text: 'heads up' }]],
      },
    ] as unknown as MarkdownBlockView[]
    const out = draw(nested)
    expect(out).toContain('<li')
    expect(out).toContain('cannot draw')
  })

  it('draws no blocks for a missing blocks value, instead of throwing', () => {
    expect(draw(null as unknown as MarkdownBlockView[])).toBe('')
    expect(draw(undefined as unknown as MarkdownBlockView[])).toBe('')
  })

  it('draws a paragraph with no runs as an empty paragraph, instead of throwing', () => {
    expect(draw([{ kind: 'paragraph', runs: null } as unknown as MarkdownBlockView])).toBe(
      '<div class="markdown"><p class="markdown__paragraph"></p></div>',
    )
    expect(draw([{ kind: 'paragraph', runs: undefined } as unknown as MarkdownBlockView])).toBe(
      '<div class="markdown"><p class="markdown__paragraph"></p></div>',
    )
  })

  it('draws a run with missing styles, text or link, instead of throwing', () => {
    const run = { text: null, styles: null, link: undefined } as unknown as MarkdownInlineView
    expect(draw(paragraph(run))).toBe(
      '<div class="markdown"><p class="markdown__paragraph"></p></div>',
    )
  })

  it('draws a link that is not a string as plain text, instead of throwing', () => {
    const run = { text: 'label', styles: [], link: 42 } as unknown as MarkdownInlineView
    expect(draw(paragraph(run))).toBe(
      '<div class="markdown"><p class="markdown__paragraph">label</p></div>',
    )
  })

  it('draws a list, quote, table, code or task with missing children, instead of throwing', () => {
    const missing = [
      { kind: 'list', ordered: false, start: 1, items: null },
      { kind: 'list', ordered: false, start: 1, items: [[null]] },
      { kind: 'quote', blocks: null },
      { kind: 'table', header: null, rows: null },
      { kind: 'code', language: 42, text: null },
      { kind: 'task', checked: true, blocks: null },
      { kind: 'heading', level: 'two', runs: null },
    ] as unknown as MarkdownBlockView[]
    expect(() => draw(missing)).not.toThrow()
    expect(draw(missing)).toContain('markdown')
  })
})
