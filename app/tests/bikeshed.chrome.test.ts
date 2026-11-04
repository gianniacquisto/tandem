import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const normalise = (text: string): string => text.replace(/\r\n/g, '\n')
const css = normalise(readFileSync(`${appRoot}renderer/src/app.css`, 'utf8'))
const face = normalise(
  readFileSync(`${appRoot}renderer/src/faces/BikeshedFace.tsx`, 'utf8'),
)
const editor = normalise(
  readFileSync(`${appRoot}renderer/src/faces/BikeshedEditor.tsx`, 'utf8'),
)

/** The declaration block for a selector, without its braces. */
function block(source: string, selector: string): string {
  const needle = `${selector} {`
  const start = source.indexOf(needle)
  if (start === -1) return ''
  const open = source.indexOf('{', start)
  const close = source.indexOf('}', open)
  if (open === -1 || close === -1) return ''
  return source.slice(open + 1, close)
}

describe('the bikeshed chrome', () => {
  it('does not repeat the face title and blurb above the tree and editor', () => {
    expect(face).not.toContain('face__title')
    expect(face).not.toContain('<Face')
    expect(face).toContain('face--wide')
  })

  it('slims the editor titlebar to one line: folder plus title plus saving', () => {
    const titlebar = block(css, '.editor__titlebar')
    expect(titlebar).toContain('28px')
    expect(titlebar).not.toContain('42px')
    expect(editor).toContain('editor__folder')
    expect(editor).toContain('editor__title')
    expect(editor).toContain('editor__saving')
  })

  it('moves backlinks and tickets into a collapsible side rail', () => {
    const main = block(css, '.bikeshed__main')
    expect(main).toContain('row')
    const rail = block(css, '.rail')
    expect(rail).toContain('column')
    expect(face).toContain('railOpen')
    expect(face).toContain('onToggleRail')
    expect(editor).toContain('editor__rail-toggle')
    expect(editor).toContain('aria-expanded')
    expect(editor).toContain('aria-controls')
  })

  it('keeps the editor at full height when the rail is collapsed', () => {
    expect(face).toContain('railOpen ?')
    expect(css).not.toContain('max-height: 168px')
  })
})
