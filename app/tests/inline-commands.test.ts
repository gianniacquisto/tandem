import { describe, expect, it } from 'vitest'

import {
  inlineKey,
  toggleInlineStyle,
  type InlineStyle,
} from '../renderer/src/shell/inline-commands'

const toggle = (source: string, from: number, to: number, style: InlineStyle) =>
  toggleInlineStyle(source, { from, to }, style)

describe('the inline style commands', () => {
  it('wraps a selection in each style and selects the text inside', () => {
    const cases: Array<[InlineStyle, string]> = [
      ['bold', '**bold**'],
      ['italic', '*italic*'],
      ['strikethrough', '~~strike~~'],
      ['code', '`code`'],
    ]
    for (const [style, expected] of cases) {
      const text = style === 'bold' ? 'bold' : style === 'italic' ? 'italic' : style === 'strikethrough' ? 'strike' : 'code'
      const result = toggle(text, 0, text.length, style)
      expect(result.source).toBe(expected)
      const width = result.selection.to - result.selection.from
      expect(result.source.slice(result.selection.from, result.selection.to)).toBe(
        result.source.slice(expected.indexOf(text), expected.indexOf(text) + width),
      )
      expect(result.selection.to - result.selection.from).toBe(text.length)
    }
  })

  it('unwraps a style that already covers the selection, and only that range changes', () => {
    for (const [style, source] of [
      ['bold', 'before **bold** after'],
      ['italic', 'before *italic* after'],
      ['strikethrough', 'before ~~strike~~ after'],
      ['code', 'before `code` after'],
    ] as Array<[InlineStyle, string]>) {
      const from = source.indexOf('**') >= 0 ? source.indexOf('**') + 2 : source.indexOf('*') >= 0 ? source.indexOf('*') + 1 : source.indexOf('~~') >= 0 ? source.indexOf('~~') + 2 : source.indexOf('`') + 1
      const inside = style === 'bold' ? 'bold' : style === 'italic' ? 'italic' : style === 'strikethrough' ? 'strike' : 'code'
      const result = toggle(source, from, from + inside.length, style)
      expect(result.source).toBe(`before ${inside} after`)
      expect(result.selection).toEqual({ from: from - (style === 'bold' || style === 'strikethrough' ? 2 : 1), to: from + inside.length - (style === 'bold' || style === 'strikethrough' ? 2 : 1) })
    }
  })

  it('applies the style to the whole selection when it covers only part of it', () => {
    const source = '**a** b'
    const result = toggle(source, 0, 5, 'bold')
    expect(result.source).toBe('****a**** b')
    expect(result.source.slice(result.selection.from, result.selection.to)).toBe('**a**')
  })

  it('inserts an empty pair at a collapsed caret and a second press removes it', () => {
    const first = toggle('ab', 1, 1, 'bold')
    expect(first.source).toBe('a****b')
    expect(first.selection).toEqual({ from: 3, to: 3 })
    const second = toggle(first.source, first.selection.from, first.selection.to, 'bold')
    expect(second.source).toBe('ab')
    expect(second.selection).toEqual({ from: 1, to: 1 })
  })

  it('removes the style around a caret inside a run and leaves a caret at the boundary alone', () => {
    const inside = toggle('**bold**', 3, 3, 'bold')
    expect(inside.source).toBe('bold')

    const atEdge = toggle('**bold**', 0, 0, 'bold')
    expect(atEdge.source).toBe('******bold**')
    expect(atEdge.selection).toEqual({ from: 2, to: 2 })

    const after = toggle('**bold**', 8, 8, 'bold')
    expect(after.source).toBe('**bold******')
    expect(after.selection).toEqual({ from: 10, to: 10 })
  })

  it('leaves every other byte of the block unchanged', () => {
    const source = 'keep **this** and   that'
    const result = toggle(source, 7, 11, 'bold')
    expect(result.source).toBe('keep this and   that')
  })
})

describe('the key mapping', () => {
  it('maps Mod+B, Mod+I and Mod+E to the three commands', () => {
    expect(inlineKey({ key: 'b', mod: true, shift: false, alt: false })).toBe('bold')
    expect(inlineKey({ key: 'i', mod: true, shift: false, alt: false })).toBe('italic')
    expect(inlineKey({ key: 'e', mod: true, shift: false, alt: false })).toBe('code')
  })

  it('leaves Mod+E to the mode when it is not a plain modifier press', () => {
    expect(inlineKey({ key: 'e', mod: false, shift: false, alt: false })).toBeNull()
    expect(inlineKey({ key: 'e', mod: true, shift: true, alt: false })).toBeNull()
    expect(inlineKey({ key: 'e', mod: true, shift: false, alt: true })).toBeNull()
    expect(inlineKey({ key: 'x', mod: true, shift: false, alt: false })).toBeNull()
  })
})
