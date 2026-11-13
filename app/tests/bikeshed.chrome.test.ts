import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const normalise = (text: string): string => text.replace(/\r\n/g, '\n')
const read = (path: string): string => normalise(readFileSync(`${appRoot}${path}`, 'utf8'))
const css = read('renderer/src/app.css')
const face = read('renderer/src/faces/BikeshedFace.tsx')
const tree = read('renderer/src/faces/BikeshedTree.tsx')
const app = read('renderer/src/App.tsx')

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

  it('keeps the note footer after the note, as the document column', () => {
    expect(face).toContain('<NoteFooter')
    expect(face).toContain('backlinks=')
    expect(face).toContain('linked=')
    expect(face).toContain('recoveries=')
    expect(css).not.toContain('.rail')
  })

  it('places the focus control inside the tree column', () => {
    const treeColumn = face.indexOf('bikeshed__tree')
    const focus = face.indexOf('<FocusControl')
    const document = face.indexOf('bikeshed__document')
    expect(treeColumn).toBeGreaterThan(-1)
    expect(focus).toBeGreaterThan(treeColumn)
    expect(document).toBeGreaterThan(focus)
  })
})

describe('the textarea editor', () => {
  it('draws a plain textarea for the whole note and nothing else', () => {
    expect(face).toContain('<textarea')
    expect(face).toContain('aria-label="Note markdown"')
    expect(face).toContain('value={open.text}')
    expect(face).toContain('applyEditor(current, event.target.value)')
  })

  it('draws the reader with the core parser and the sheet renderer', () => {
    expect(face).toContain("method: 'markdown.blocks'")
    expect(face).toContain('<MarkdownDocument blocks={blocks} />')
    expect(face).toContain('<div className="note-preview"')
  })

  it('switches between the reader and the editor with one control', () => {
    expect(face).toContain('bikeshed__mode')
    expect(face).toContain("{mode === 'edit' ? 'Done' : 'Edit'}")
    expect(face).toContain('onClick={() => void toggleMode()}')
  })

  it('draws the editor on the paper and ink tokens, monospaced', () => {
    const editor = block(css, '.note-editor')
    expect(editor).toContain('var(--ink)')
    expect(editor).toContain('ui-monospace')
    expect(editor).toContain('resize: none')
    expect(block(css, '.note-preview')).toContain('var(--ink)')
  })

  it('keeps the mode control off the note body', () => {
    const control = block(css, '.bikeshed__mode')
    expect(control).toContain('var(--paper)')
    expect(control).toContain('var(--line)')
    expect(control).not.toContain('--tandem-editor-font-size')
  })
})

describe('the cut block editor', () => {
  const retired = [
    'renderer/src/faces/BikeshedBlocks.tsx',
    'renderer/src/faces/BikeshedToolbar.tsx',
    'renderer/src/faces/InlineRuns.tsx',
    'renderer/src/faces/NoteSizeControl.tsx',
    'renderer/src/shell/blocks.ts',
    'renderer/src/shell/editor-ops.ts',
    'renderer/src/shell/inline.ts',
    'renderer/src/shell/inline-commands.ts',
    'renderer/src/shell/toolbar.ts',
    'renderer/src/shell/fontSize.ts',
  ]

  it('is gone, file for file', () => {
    for (const path of retired) {
      expect(existsSync(`${appRoot}${path}`), path).toBe(false)
    }
    expect(existsSync(`${appRoot}tests/fixtures/bikeshed-corpus`)).toBe(false)
  })

  it('is named nowhere in the source tree', () => {
    for (const file of sourceFiles(`${appRoot}renderer/src`)) {
      const text = normalise(readFileSync(file, 'utf8'))
      for (const retiredModule of [
        './blocks',
        '../shell/blocks',
        './editor-ops',
        '../shell/editor-ops',
        './inline',
        '../shell/inline',
        './toolbar',
        '../shell/toolbar',
        './fontSize',
        '../shell/fontSize',
        'BikeshedBlocks',
        'BikeshedToolbar',
        'InlineRuns',
        'NoteSizeControl',
      ]) {
        expect(text, `${file} still names ${retiredModule}`).not.toContain(retiredModule)
      }
    }
  })

  it('carries no @lezer/markdown and no block CSS', () => {
    expect(read('package.json')).not.toContain('@lezer/markdown')
    for (const selector of ['.blocks', '.block', '.health', '.editor__size', '.bikeshed__toolbar']) {
      expect(css, selector).not.toContain(`${selector} {`)
    }
  })

  it('retires the memory chrome with the memory layer', () => {
    expect(face).not.toContain('note.health')
    expect(face).not.toContain('note.daily')
    expect(face).not.toContain('tandem.bikeshed.hidden')
    expect(tree).not.toContain('Show hidden')
    expect(tree).not.toContain("Today's note")
    expect(tree).not.toContain('healthSummary')
    expect(tree).not.toContain('className="health')
  })
})
