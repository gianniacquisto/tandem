import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const tokensPath = `${appRoot}renderer/src/tokens.css`

/** A Windows checkout reads the same stylesheet with CRLF endings. */
const normalise = (text: string) => text.replace(/\r\n/g, '\n')
const css = normalise(readFileSync(tokensPath, 'utf8'))

/** `accentDeep` in Swift became `accent-deep` in CSS. */
const variable = (name: string) => `--${name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`

/**
 * The palette the Swift theme carried, frozen here at the cutover: the Swift
 * file is gone, so the hex values are the source of truth and this table is the
 * only place a drift can be caught.
 */
const DAY = new Map<string, string>([
  ['paper', '#F1E9D7'],
  ['parchment', '#E7DECA'],
  ['sand', '#D6C9AE'],
  ['shadow', '#C6B897'],
  ['line', '#C6B99E'],
  ['ink', '#211D18'],
  ['inkDim', '#877D6B'],
  ['accent', '#6E7D6E'],
  ['accentDeep', '#55604F'],
  ['ember', '#A9745C'],
  ['emberDeep', '#8B5A45'],
  ['moss', '#6C7A66'],
  ['mossDeep', '#7E8D77'],
  ['sky', '#CFDDE2'],
  ['hill', '#B7C4AE'],
  ['hillFar', '#C8CFBB'],
  ['skin', '#E4C9A2'],
  ['sun', '#E5C07B'],
])

const NIGHT = new Map<string, string>([
  ['paper', '#23211C'],
  ['parchment', '#2C2A23'],
  ['sand', '#3C3930'],
  ['shadow', '#191712'],
  ['line', '#494437'],
  ['ink', '#EDE6D3'],
  ['inkDim', '#9C9583'],
  ['accent', '#93A48F'],
  ['accentDeep', '#6E7E6A'],
  ['ember', '#C08A70'],
  ['emberDeep', '#9E6B54'],
  ['moss', '#8FA084'],
  ['mossDeep', '#74856E'],
  ['sky', '#2A2E28'],
  ['hill', '#3A4441'],
  ['hillFar', '#313A36'],
  ['skin', '#E3C29B'],
  ['sun', '#EDE6D3'],
])

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

function lightBlock(text = css): string {
  const match = text.match(/:root\s*\{([\s\S]*?)\n\}/)
  if (!match?.[1]) throw new Error('no :root block in tokens.css')
  return match[1]
}

function darkBlock(text = css): string {
  const match = text.match(/@media \(prefers-color-scheme: dark\)\s*\{\s*:root\s*\{([\s\S]*?)\n  \}/)
  if (!match?.[1]) throw new Error('no dark :root block in tokens.css')
  return match[1]
}

/** The day rule's shape and value, on whichever checkout's text it is given. */
function checkDayRoot(text: string): void {
  const rule = text.indexOf(':root')
  expect(rule).toBeGreaterThan(0)

  const header = text.slice(0, rule).trim()
  expect(header.startsWith('/*')).toBe(true)
  expect(header.endsWith('*/')).toBe(true)
  expect(header.slice(2, -2)).not.toContain('*/')

  expect(text.slice(rule)).toMatch(/^:root \{\n[\s\S]*?\n\}/)
  expect(declarations(lightBlock(text)).get('paper')).toBe('#F1E9D7')
}

describe('the design tokens', () => {
  it('opens with one closed header comment, so the day :root rule parses', () => {
    checkDayRoot(css)
  })

  it('holds on a CRLF checkout of the same stylesheet', () => {
    const crlf = normalise(readFileSync(tokensPath, 'utf8')).replace(/\n/g, '\r\n')
    expect(crlf).toContain('\r\n')
    expect(crlf).not.toMatch(/[^\r]\n/)

    const text = normalise(crlf)
    expect(text).toBe(css)
    checkDayRoot(text)
  })

  it('keeps every day palette entry, value for value', () => {
    const day = DAY
    expect(day.size).toBe(18)

    const declared = declarations(lightBlock())
    for (const [name, hex] of day) {
      expect(declared.get(variable(name).slice(2)), variable(name)).toBe(hex)
    }
  })

  it('keeps every night palette entry, value for value', () => {
    const night = NIGHT
    expect(night.size).toBe(18)

    const declared = declarations(darkBlock())
    for (const [name, hex] of night) {
      expect(declared.get(variable(name).slice(2)), variable(name)).toBe(hex)
    }
  })

  it('declares no palette variable the theme does not have', () => {
    const theme = new Set([...DAY.keys()].map((name) => variable(name).slice(2)))
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
