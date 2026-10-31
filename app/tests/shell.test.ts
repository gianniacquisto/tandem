import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { TABS } from '../renderer/src/shell/tabs'
import { parseRoute, tabHref, viewHref } from '../renderer/src/shell/route'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const renderer = join(appRoot, 'renderer')

function filesUnder(directory: string, extensions: string[]): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry)
    if (statSync(path).isDirectory()) return filesUnder(path, extensions)
    return extensions.some((extension) => path.endsWith(extension)) ? [path] : []
  })
}

// `public/` is the committed editor bundle, copied byte for byte from the Swift
// resource: its colours and its own CSS are not the app's to rewrite, and
// `tests/bundle.test.ts` is the test that keeps it verbatim.
const sources = filesUnder(renderer, ['.ts', '.tsx', '.css', '.html']).filter(
  (path) => !path.includes(`${join('renderer', 'public')}`),
)

describe('the shell', () => {
  it('keeps the four tabs, in the spec order', () => {
    expect(TABS.map((tab) => tab.label)).toEqual(['Tickets', 'Bikeshed', 'Usage', 'Settings'])
    expect(new Set(TABS.map((tab) => tab.id)).size).toBe(4)
  })

  it('routes every tab to its own path and back', () => {
    for (const tab of TABS) {
      expect(parseRoute(tabHref(tab.id))).toEqual({ tab: tab.id, view: 'list' })
      expect(tabHref(tab.id)).toBe(`#/${tab.id}`)
    }
    expect(parseRoute('')).toEqual({ tab: 'tickets', view: 'list' })
    expect(parseRoute('#/nowhere')).toEqual({ tab: 'tickets', view: 'list' })
  })

  it('routes the Tickets face two ways: the list and the board', () => {
    expect(parseRoute(viewHref('tickets', 'list'))).toEqual({ tab: 'tickets', view: 'list' })
    expect(parseRoute(viewHref('tickets', 'board'))).toEqual({ tab: 'tickets', view: 'board' })
    expect(viewHref('tickets', 'board')).toBe('#/tickets/board')
    // Only the Tickets face has a board; another tab's second segment is ignored.
    expect(parseRoute('#/usage/board')).toEqual({ tab: 'usage', view: 'list' })
  })

  it('keeps Node out of the renderer: no electron, no builtin, no process', () => {
    const forbidden = [
      /from\s+['"]electron['"]/,
      /from\s+['"]node:/,
      /from\s+['"](fs|path|child_process|os|net|http|https|util|crypto)['"]/,
      /\brequire\s*\(/,
      /\bprocess\./,
    ]

    for (const path of filesUnder(renderer, ['.ts', '.tsx'])) {
      if (path.includes(`${join('renderer', 'src', 'wire')}`)) continue
      const source = readFileSync(path, 'utf8')
      for (const pattern of forbidden) {
        expect(source, `${path} matches ${pattern}`).not.toMatch(pattern)
      }
    }
  })

  it('keeps every colour in tokens.css: no literal in a component', () => {
    const literal = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/

    for (const path of sources) {
      if (path.endsWith('tokens.css')) continue
      expect(readFileSync(path, 'utf8'), `${path} carries a colour literal`).not.toMatch(literal)
    }
  })

  it('enforces the window floor in the document as well as the window', () => {
    const css = readFileSync(join(renderer, 'src', 'app.css'), 'utf8')
    expect(css).toMatch(/min-width:\s*var\(--window-floor\)/)
  })

  it('exposes one world key, named tandem', () => {
    const preload = readFileSync(join(appRoot, 'src', 'preload', 'index.ts'), 'utf8')
    const worlds = [...preload.matchAll(/exposeInMainWorld\(\s*['"]([^'"]+)['"]/g)]
    expect(worlds.map((match) => match[1])).toEqual(['tandem'])
  })
})
