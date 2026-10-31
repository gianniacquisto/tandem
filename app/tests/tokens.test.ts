import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const repoRoot = fileURLToPath(new URL('../..', import.meta.url))

const swift = readFileSync(`${repoRoot}Sources/Tandem/LigneTheme.swift`, 'utf8')
const css = readFileSync(`${appRoot}renderer/src/tokens.css`, 'utf8')

/** `accentDeep` in Swift is `accent-deep` in CSS. */
const variable = (name: string) => `--${name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`

/** The palette's entries in the Swift theme, as name to hex. */
function palette(name: 'day' | 'night'): Map<string, string> {
  const block = swift.match(new RegExp(`static let ${name} = Palette\\(([\\s\\S]*?)\\n    \\)`))
  if (!block?.[1]) throw new Error(`no ${name} palette in LigneTheme.swift`)

  const entries = new Map<string, string>()
  for (const match of block[1].matchAll(/(\w+):\s*hex\(0x([0-9A-Fa-f]{6})\)/g)) {
    const key = match[1]
    const hex = match[2]
    if (key === undefined || hex === undefined) continue
    entries.set(key, `#${hex.toUpperCase()}`)
  }
  return entries
}

/** The variables a CSS block declares, as name to value. */
function declarations(block: string): Map<string, string> {
  const entries = new Map<string, string>()
  for (const match of block.matchAll(/--([\w-]+):\s*([^;]+);/g)) {
    const name = match[1]
    const value = match[2]
    if (name === undefined || value === undefined) continue
    entries.set(name, value.trim())
  }
  return entries
}

function lightBlock(): string {
  const match = css.match(/:root\s*\{([\s\S]*?)\n\}/)
  if (!match?.[1]) throw new Error('no :root block in tokens.css')
  return match[1]
}

function darkBlock(): string {
  const match = css.match(/@media \(prefers-color-scheme: dark\)\s*\{\s*:root\s*\{([\s\S]*?)\n  \}/)
  if (!match?.[1]) throw new Error('no dark :root block in tokens.css')
  return match[1]
}

describe('the design tokens', () => {
  it('ports every day palette entry from the Swift theme, value for value', () => {
    const day = palette('day')
    expect(day.size).toBe(18)

    const declared = declarations(lightBlock())
    for (const [name, hex] of day) {
      expect(declared.get(variable(name).slice(2)), variable(name)).toBe(hex)
    }
  })

  it('ports every night palette entry, value for value', () => {
    const night = palette('night')
    expect(night.size).toBe(18)

    const declared = declarations(darkBlock())
    for (const [name, hex] of night) {
      expect(declared.get(variable(name).slice(2)), variable(name)).toBe(hex)
    }
  })

  it('declares no palette variable the theme does not have', () => {
    const theme = new Set([...palette('day').keys()].map((name) => variable(name).slice(2)))
    const scales = new Set([
      'hairline',
      'radius-checkbox',
      'radius-control',
      'radius-card',
      'space-gutter',
      'space-row',
      'space-control',
      'space-lane',
      'motion-insert',
      'motion-toggle',
      'motion-remove',
      'window-floor',
    ])

    for (const name of declarations(lightBlock()).keys()) {
      expect(theme.has(name) || scales.has(name), `--${name} is not a token`).toBe(true)
    }
  })

  it('ports the shared scale: hairline, radii, spacing, motion', () => {
    const tokens = declarations(lightBlock())

    expect(tokens.get('hairline')).toBe('1px')
    expect(tokens.get('radius-checkbox')).toBe('5px')
    expect(tokens.get('radius-control')).toBe('9px')
    expect(tokens.get('radius-card')).toBe('12px')
    expect(tokens.get('space-gutter')).toBe('20px')
    expect(tokens.get('space-row')).toBe('40px')
    expect(tokens.get('space-control')).toBe('32px')
    expect(tokens.get('space-lane')).toBe('12px')
    expect(tokens.get('motion-insert')).toBe('200ms')
    expect(tokens.get('motion-toggle')).toBe('180ms')
    expect(tokens.get('motion-remove')).toBe('150ms')
    expect(tokens.get('window-floor')).toBe('760px')
  })

  it('follows the system appearance rather than forcing one', () => {
    expect(css).toMatch(/@media \(prefers-color-scheme: dark\)/)
    expect(css).not.toMatch(/prefers-color-scheme:\s*light/)
  })
})
