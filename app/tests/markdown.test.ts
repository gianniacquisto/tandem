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
