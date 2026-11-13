import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const renderer = join(appRoot, 'renderer', 'src')
const css = readFileSync(join(renderer, 'app.css'), 'utf8')
const tokens = readFileSync(join(renderer, 'tokens.css'), 'utf8')
const app = readFileSync(join(renderer, 'App.tsx'), 'utf8')
const face = readFileSync(join(renderer, 'faces', 'Face.tsx'), 'utf8')

function faceSource(name: string): string {
  return readFileSync(join(renderer, 'faces', name), 'utf8')
}

/** One rule's body for a literal selector, or null when the rule is absent. */
function rule(selector: string): string | null {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return css.match(new RegExp(`${escaped}\\s*\\{([\\s\\S]*?)\\n\\}`, 'm'))?.[1] ?? null
}

describe('the shell chrome (TAND-16)', () => {
  it('shows wordmark plus tabs only in the header, no rev', () => {
    expect(app).toContain('shell__wordmark')
    expect(app).toContain('<nav className="tabs"')
    expect(app).not.toContain('shell__revision')
  })

  it('repeats no tab as a face heading', () => {
    expect(face).not.toContain('<h1')
    expect(face).not.toContain('face__title')
    for (const name of ['TicketsFace.tsx', 'BikeshedFace.tsx']) {
      const source = faceSource(name)
      expect(source, `${name} repeats its tab as a heading`).not.toContain('face__title')
      expect(source, `${name} repeats its tab as a heading`).not.toContain('<h1')
    }
  })

  it('reads Settings as a centred 780px column', () => {
    const settingsColumn = rule('.settings')
    expect(settingsColumn, '.settings centres the face as a column').toContain('780px')
    expect(settingsColumn, '.settings centres the face as a column').toMatch(
      /margin:\s*0\s+auto/,
    )
  })

  it('keeps the board and Bikeshed on the full width', () => {
    const board = rule('.tickets--board')
    expect(board, '.tickets--board keeps the wide rule').toContain('max-width: none')

    const wide = rule('.face--wide')
    expect(wide, '.face--wide stays uncapped').toContain('max-width: none')

    expect(faceSource('BikeshedFace.tsx')).toContain('face--wide')
  })

  it('reuses the TicketsFace wide rule in app.css instead of a new token', () => {
    expect(tokens).not.toContain('780')
  })
})
