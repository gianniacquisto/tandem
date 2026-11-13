import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { parseDocument, type Document } from '../renderer/src/shell/blocks'
import { BikeshedBlocks, BlockMenu, BlockShell } from '../renderer/src/faces/BikeshedBlocks'

function draw(text: string): string {
  return renderToStaticMarkup(
    createElement(BikeshedBlocks, {
      document: parseDocument(text),
      focusToken: 0,
      onChange: () => undefined,
    }),
  )
}

function drawWith({
  text,
  readOnly = false,
}: {
  text: string
  readOnly?: boolean
}): string {
  return renderToStaticMarkup(
    createElement(BikeshedBlocks, {
      document: parseDocument(text),
      focusToken: 0,
      readOnly,
      onChange: () => undefined,
    }),
  )
}

function drawDocument(document: Document): string {
  return renderToStaticMarkup(
    createElement(BikeshedBlocks, {
      document,
      focusToken: 0,
      onChange: () => undefined,
    }),
  )
}

describe('the Write surface draws each block kind', () => {
  it('draws a paragraph with its text', () => {
    const markup = draw('A plain paragraph.')
    expect(markup).toContain('block block--paragraph')
    expect(markup).toContain('A plain paragraph.')
  })

  it('draws an h1 to h3 with its level and text', () => {
    const markup = draw('# One\n\n## Two\n\n### Three')
    expect(markup).toContain('block block--heading')
    expect(markup).toContain('data-level="1"')
    expect(markup).toContain('data-level="2"')
    expect(markup).toContain('data-level="3"')
    expect(markup).toContain('One')
    expect(markup).toContain('Two')
    expect(markup).toContain('Three')
  })

  it('draws a bullet and an ordered list with markers', () => {
    const bullet = draw('- one\n- two')
    expect(bullet).toContain('block block--list')
    expect(bullet).toContain('block--bullet')
    expect(bullet).toContain('one')
    expect(bullet).toContain('two')

    const ordered = draw('3. three\n4. four')
    expect(ordered).toContain('block--ordered')
    expect(ordered).toContain('3. ')
    expect(ordered).toContain('4. ')
  })

  it('draws a task list with its checkbox state', () => {
    const markup = draw('- [ ] todo\n- [x] done')
    expect(markup).toContain('block block--task')
    expect(markup).toContain('aria-checked="false"')
    expect(markup).toContain('aria-checked="true"')
    expect(markup).toContain('todo')
    expect(markup).toContain('done')
  })

  it('draws a quote with its line markers', () => {
    const markup = draw('> one\n> two')
    expect(markup).toContain('block block--quote')
    expect(markup).toContain('one')
    expect(markup).toContain('two')
  })

  it('draws a code block with its language field and body', () => {
    const markup = draw('```ts\nconst answer = 42\n```')
    expect(markup).toContain('block block--code')
    expect(markup).toContain('block__code-lang')
    expect(markup).toContain('value="ts"')
    expect(markup).toContain('const answer = 42')
  })

  it('draws a rule', () => {
    expect(draw('---')).toContain('block block--rule')
  })

  it('draws an image as its placeholder with alt and target, never a load', () => {
    for (const target of [
      'pics/a#b.png',
      'https://example.com/x.png',
      '../../escape.png',
      'C:\\evil.png',
      '/',
    ]) {
      const markup = draw(`![A diagram](${target})`)
      expect(markup, target).toContain('block block--image')
      expect(markup, target).toContain('block__image-placeholder')
      expect(markup, target).toContain('A diagram')
      expect(markup, target).toContain('data-kind="image"')
      expect(markup, target).not.toContain('<img')
      expect(markup, target).not.toContain('tandem:')
      expect(markup, target).toMatch(/contenteditable="false"/i)
    }
  })

  it('draws an empty note as one paragraph', () => {
    const markup = drawDocument(parseDocument(''))
    expect(markup).toContain('block block--paragraph')
  })
})

describe('the opaque block', () => {
  it('draws an unmodelled construct as its exact source with a markdown escape', () => {
    const markup = draw('| a | b |\n| - | - |')
    expect(markup).toContain('block block--opaque')
    expect(markup).toContain('| a | b |')
    expect(markup).toContain('Edit as Markdown')
  })

  it('draws frontmatter read-only, outside the editable text', () => {
    const markup = draw('---\ntitle: A note\n---\n\nA paragraph.')
    expect(markup).toContain('block block--opaque')
    expect(markup).toContain('title: A note')
    expect(markup).toContain('Edit as Markdown')
  })
})

describe('the block surface draws the handles and the menu', () => {
  it('draws a plus and a grip on every block, the read-only card included', () => {
    const markup = draw('one\n\n- a\n- b\n\n---\n\n| a | b |\n| - | - |')
    expect(markup.match(/block__handles/g)).toHaveLength(4)
    expect(markup.match(/aria-label="Add a block below"/g)).toHaveLength(4)
    expect(
      markup.match(/aria-label="Drag to reorder; Mod\+Shift\+Arrow moves it"/g),
    ).toHaveLength(4)
    expect(markup.match(/draggable="true"/g)).toHaveLength(4)
    // The read-only card is a block like any other: it takes both handles.
    expect(markup).toContain('block block--opaque')
  })

  it('draws no handles on a read-only surface', () => {
    const markup = drawWith({ text: 'one\n\ntwo', readOnly: true })
    expect(markup).toContain('block block--paragraph')
    expect(markup).not.toContain('block__handles')
    expect(markup).not.toContain('block__menu')
  })

  it('draws the listbox with its rows, the header and the selected row', () => {
    const markup = renderToStaticMarkup(
      createElement(BlockMenu, { query: '', active: 2, onChoose: () => undefined }),
    )
    expect(markup).toContain('role="listbox"')
    expect(markup).toContain('aria-label="Blocks"')
    expect(markup).toContain('Blocks')
    expect(markup.match(/role="option"/g)).toHaveLength(10)
    expect(markup.match(/aria-selected="true"/g)).toHaveLength(1)
    expect(markup.match(/block__menu-row--active/g)).toHaveLength(1)

    const filtered = renderToStaticMarkup(
      createElement(BlockMenu, { query: 'h2', active: 0, onChoose: () => undefined }),
    )
    expect(filtered.match(/role="option"/g)).toHaveLength(1)
    expect(filtered).toContain('Heading 2')
  })

  it('draws the no-match row and keeps the listbox', () => {
    const markup = renderToStaticMarkup(
      createElement(BlockMenu, { query: 'nothing here', active: 0, onChoose: () => undefined }),
    )
    expect(markup).toContain('role="listbox"')
    expect(markup).toContain('No blocks match')
    expect(markup.match(/role="option"/g)).toBeNull()
  })

  it('draws the drop indicator above and below the block', () => {
    const above = renderToStaticMarkup(
      createElement(BlockShell, { index: 0, kind: 'paragraph', indicator: 'above' }, 'text'),
    )
    expect(above).toContain('block-shell--drop-above')
    expect(above).toContain('block__drop')

    const below = renderToStaticMarkup(
      createElement(BlockShell, { index: 1, kind: 'paragraph', indicator: 'below' }, 'text'),
    )
    expect(below).toContain('block-shell--drop-below')
    expect(below).toContain('block__drop')

    const none = renderToStaticMarkup(
      createElement(BlockShell, { index: 0, kind: 'paragraph' }, 'text'),
    )
    expect(none).toContain('block-shell')
    expect(none).not.toContain('block__drop')
  })
})
