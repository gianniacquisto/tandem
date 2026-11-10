import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const normalise = (text: string): string => text.replace(/\r\n/g, '\n')
const css = normalise(readFileSync(`${appRoot}renderer/src/app.css`, 'utf8'))
const face = normalise(
  readFileSync(`${appRoot}renderer/src/faces/BikeshedFace.tsx`, 'utf8'),
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
    expect(face).not.toContain('editor__rail-toggle')
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

/** Every file under a folder, recursively. */
function sourceFiles(directory: string): string[] {
  const out: string[] = []
  for (const entry of readdirSync(directory)) {
    const path = join(directory, entry)
    if (statSync(path).isDirectory()) out.push(...sourceFiles(path))
    else out.push(path)
  }
  return out
}

describe('the one editor', () => {
  const blocks = normalise(
    readFileSync(`${appRoot}renderer/src/faces/BikeshedBlocks.tsx`, 'utf8'),
  )
  const shellEditor = normalise(readFileSync(`${appRoot}renderer/src/shell/fontSize.ts`, 'utf8'))

  it('draws no mode control and keeps no mode memory', () => {
    expect(face).not.toContain('editor__mode')
    expect(face).not.toContain('SourceSurface')
    expect(face).not.toContain('loadMode')
    expect(face).not.toContain('saveMode')
    expect(face).not.toContain('requestMode')
    expect(face).not.toContain('Preview')
    expect(existsSync(`${appRoot}renderer/src/faces/SourceSurface.tsx`)).toBe(false)
    expect(existsSync(`${appRoot}renderer/src/shell/modes.ts`)).toBe(false)
    expect(css).not.toContain('.source')
    expect(css).not.toContain('.editor__mode')
  })

  it('draws the block surface as the only surface, with no mode switch', () => {
    expect(face).toContain('<BikeshedBlocks')
    expect(face).not.toContain('<SourceSurface')
    expect(face).not.toContain('mode ===')
  })

  it('points an opaque block at in-place markdown, not a mode switch', () => {
    expect(blocks).toContain('Edit as Markdown')
    expect(blocks).toContain('onMarkdownStart')
    expect(blocks).toContain('commitBlockSource')
    expect(blocks).not.toContain('onSwitchToSource')
    expect(blocks).not.toContain('Edit in Source mode')
  })

  it('draws the raw markdown block as a textarea in place', () => {
    expect(blocks).toContain('<textarea')
    expect(blocks).toContain('aria-label="Edit block as markdown"')
    expect(blocks).toContain('onMarkdownCommit')
  })

  it('retires the preview vocabulary from the shell', () => {
    expect(shellEditor).not.toMatch(
      /preview|Preview|BikeshedMode|BIKESHED_MODE|defaultBikeshedMode|editorMode|toggle-mode/,
    )
  })

  it('imports no CodeMirror', () => {
    expect(readFileSync(`${appRoot}package.json`, 'utf8')).not.toContain('@codemirror')
    for (const file of sourceFiles(`${appRoot}renderer/src`)) {
      expect(readFileSync(file, 'utf8')).not.toContain('@codemirror')
    }
  })

  it('reads the in-place markdown on the paper and ink tokens', () => {
    const area = block(css, '.block__markdown-source')
    expect(area).toContain('var(--ink)')
    expect(area).toContain('ui-monospace')
    expect(area).toContain('var(--tandem-editor-font-size')
  })
})

describe('the formatting toolbar chrome', () => {
  const toolbar = normalise(
    readFileSync(`${appRoot}renderer/src/faces/BikeshedToolbar.tsx`, 'utf8'),
  )

  it('the toolbar row is pinned under the breadcrumb', () => {
    const row = block(css, '.bikeshed__toolbar')
    expect(row).toContain('position: sticky')
    expect(row).toMatch(/top:\s*40px/)
    expect(row).toContain('var(--paper)')
    expect(row).toContain('var(--sand)')
    expect(row).not.toMatch(/#[0-9a-fA-F]{3,8}/)
    expect(row).toContain('font-size: 12px')

    const button = block(css, '.bikeshed__toolbar-button')
    expect(button).toContain('var(--ink-dim)')
    expect(block(css, '.bikeshed__toolbar-button[aria-pressed="true"]')).toContain(
      'var(--parchment)',
    )
    expect(block(css, '.bikeshed__toolbar-button[aria-pressed="true"]')).toContain(
      'var(--radius-control)',
    )
  })

  it('draws inline SVG glyphs and never follows the note font size', () => {
    expect(toolbar).toContain('<svg')
    expect(toolbar).not.toContain('dangerouslySetInnerHTML')
    expect(toolbar).not.toContain('--tandem-editor-font-size')
  })
})

/** The declaration block at a selector, including a grouped selector's tail. */
function ruleAt(source: string, selector: string): string {
  const start = source.indexOf(selector)
  if (start === -1) return ''
  const open = source.indexOf('{', start)
  const close = source.indexOf('}', open)
  if (open === -1 || close === -1) return ''
  return source.slice(open + 1, close)
}

describe('the note size control', () => {
  const control = normalise(
    readFileSync(`${appRoot}renderer/src/faces/NoteSizeControl.tsx`, 'utf8'),
  )
  const blocks = normalise(
    readFileSync(`${appRoot}renderer/src/faces/BikeshedBlocks.tsx`, 'utf8'),
  )

  it('draws the size group at the end of the header, with no mode group', () => {
    const headerStart = face.indexOf('<header className="bikeshed__breadcrumb">')
    const headerEnd = face.indexOf('</header>', headerStart)
    expect(headerStart).toBeGreaterThan(-1)
    const header = face.slice(headerStart, headerEnd)
    expect(header.indexOf('<NoteSizeControl')).toBeGreaterThan(-1)
    expect(header).not.toContain('editor__mode')
  })

  it('hides the size group with no note open', () => {
    const header = face.indexOf('<header className="bikeshed__breadcrumb">')
    const guard = face.lastIndexOf('open === null ? null : (', header)
    expect(header).toBeGreaterThan(-1)
    expect(guard).toBeGreaterThan(-1)
    expect(guard).toBeLessThan(header)
  })

  it('names the group, both buttons and the live numeric readout', () => {
    expect(control).toContain('role="group"')
    expect(control).toContain('aria-label="Note size"')
    expect(control).toContain('aria-label="Decrease note font size"')
    expect(control).toContain('aria-label="Increase note font size"')
    expect(control).toContain('aria-live="polite"')
    expect(control).toContain('{size}')
  })

  it('disables each button at its bound', () => {
    expect(control).toContain('disabled={size <= EDITOR_FONT_SIZE_MIN}')
    expect(control).toContain('disabled={size >= EDITOR_FONT_SIZE_MAX}')
  })

  it('holds one size loaded from the key and persists a step', () => {
    expect(face).toContain('useState(() => loadEditorFontSize())')
    expect(face).toContain('stepEditorFontSize(size, delta)')
    expect(face).toContain('saveEditorFontSize(next)')
  })

  it('sets the note size once on the wrapper the surface sits in', () => {
    const applied = face.match(/--tandem-editor-font-size/g) ?? []
    expect(applied).toHaveLength(1)
    expect(face).toContain("'--tandem-editor-font-size': `${size}px`")
    expect(blocks).not.toContain('--tandem-editor-font-size')
  })

  it('leaves the surface off the stored key and without a size prop', () => {
    expect(blocks).not.toContain('loadEditorFontSize')
    expect(blocks).not.toContain('fontSize={')
    expect(blocks).not.toMatch(/fontSize:\s*number/)
    expect(block(css, '.blocks')).toContain('var(--tandem-editor-font-size')
    expect(block(css, '.block__markdown-source')).toContain('var(--tandem-editor-font-size')
  })

  it("retires the bundle's font-size message and page global", () => {
    for (const file of sourceFiles(`${appRoot}renderer/src`)) {
      const text = normalise(readFileSync(file, 'utf8'))
      expect(text).not.toContain('setFontSize')
      expect(text).not.toMatch(/type:\s*['"]font-size['"]/)
    }
  })

  it('draws the control on tokens and never at the note size', () => {
    for (const selector of ['.editor__size', '.editor__size-button', '.editor__size-readout']) {
      const rule = ruleAt(css, selector)
      expect(rule, selector).not.toBe('')
      expect(rule, selector).toMatch(/var\(--/)
      expect(rule, selector).not.toMatch(/#[0-9a-f]{3,8}|rgba?\(/i)
      expect(rule, selector).not.toContain('--tandem-editor-font-size')
    }
  })
})
