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
const app = normalise(readFileSync(`${appRoot}renderer/src/App.tsx`, 'utf8'))

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
  it('draws no card and takes the shell bleed on the bikeshed route alone', () => {
    const card = block(css, '.bikeshed')
    expect(card).not.toMatch(/(^|\s)border\s*:/)
    expect(card).not.toMatch(/border-radius/)

    const bleed = block(css, '.shell__body--bleed')
    expect(bleed).toMatch(/padding:\s*0/)
    expect(bleed).toMatch(/border-top:\s*0/)

    expect(app).toContain(
      "route.tab === 'bikeshed' ? 'shell__body shell__body--bleed' : 'shell__body'",
    )
  })

  it('fixes the tree at 260 and centres the document at 740', () => {
    expect(block(css, '.bikeshed__tree')).toContain('flex: 0 0 260px')
    const page = block(css, '.bikeshed__page')
    expect(page).toContain('max-width: 740px')
    expect(page).toMatch(/margin:\s*0\s+auto/)
  })

  it('sticks the breadcrumb over a scrolling document column', () => {
    const breadcrumb = block(css, '.bikeshed__breadcrumb')
    expect(breadcrumb).toContain('position: sticky')
    expect(breadcrumb).toContain('top: 0')
    expect(block(css, '.bikeshed__document')).toContain('overflow-y: auto')
  })

  it('moves the rail into the document footer and retires its toggle', () => {
    expect(face).toContain('<NoteFooter')
    expect(face).toContain('backlinks=')
    expect(face).toContain('linked=')
    expect(face).toContain('recoveries=')
    expect(face).not.toContain('railOpen')
    expect(face).not.toContain('onToggleRail')
    expect(face).not.toContain('tandem.bikeshed.rail-open')
    expect(editor).not.toContain('editor__rail-toggle')
    expect(css).not.toContain('.rail')
  })

  it('keeps the shell error row boxed so the rules after it parse', () => {
    const error = block(css, '.shell__error')
    expect(error).toContain('padding: 10px 14px')
    expect(error).toContain('border: var(--hairline) solid var(--ember)')
    expect(error).toContain('max-width: 540px')
    // The block sits above `.face`: an orphaned one takes that rule with it.
    expect(block(css, '.face')).toContain('max-width: 540px')
  })

  it('places the focus control inside the tree column', () => {
    const tree = face.indexOf('bikeshed__tree')
    const focus = face.indexOf('<FocusControl')
    const document = face.indexOf('bikeshed__document')
    expect(tree).toBeGreaterThan(-1)
    expect(focus).toBeGreaterThan(tree)
    expect(document).toBeGreaterThan(focus)
  })
})
