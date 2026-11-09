import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { parseDocument, type Document } from '../renderer/src/shell/blocks'
import { BikeshedBlocks } from '../renderer/src/faces/BikeshedBlocks'

function draw(text: string): string {
  return renderToStaticMarkup(
    createElement(BikeshedBlocks, {
      document: parseDocument(text),
      focusToken: 0,
      fontSize: 16,
      onChange: () => undefined,
      onSendToTodos: () => undefined,
      onSwitchToSource: () => undefined,
    }),
  )
}

function drawDocument(document: Document): string {
  return renderToStaticMarkup(
    createElement(BikeshedBlocks, {
      document,
      focusToken: 0,
      fontSize: 16,
      onChange: () => undefined,
      onSendToTodos: () => undefined,
      onSwitchToSource: () => undefined,
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

  it('draws the image placeholder with alt and target and fetches nothing', () => {
    const markup = draw('![A diagram](pics/diagram.png)')
    expect(markup).toContain('block block--image')
    expect(markup).toContain('A diagram')
    expect(markup).toContain('pics/diagram.png')
    expect(markup).not.toContain('<img')
  })

  it('draws an empty note as one paragraph', () => {
    const markup = drawDocument(parseDocument(''))
    expect(markup).toContain('block block--paragraph')
  })
})

describe('the read-only source card', () => {
  it('draws an unmodelled construct as its exact source and points at the raw editor', () => {
    const markup = draw('| a | b |\n| - | - |')
    expect(markup).toContain('block block--opaque')
    expect(markup).toContain('| a | b |')
    expect(markup).toContain('Edit this in the raw editor')
  })

  it('draws frontmatter read-only, outside the editable text', () => {
    const markup = draw('---\ntitle: A note\n---\n\nA paragraph.')
    expect(markup).toContain('block block--opaque')
    expect(markup).toContain('title: A note')
    expect(markup).toContain('Edit this in the raw editor')
  })
})
