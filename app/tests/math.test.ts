import { describe, expect, it } from 'vitest'

import { renderMath } from '../renderer/src/shell/math'

describe('the math render', () => {
  it('draws a valid inline run as KaTeX', () => {
    const html = renderMath('x^2', false)

    expect(html).toContain('class="katex"')
    expect(html).not.toContain('katex-display')
  })

  it('draws a display block in display mode', () => {
    const html = renderMath('x = 1', true)

    expect(html).toContain('class="katex"')
    expect(html).toContain('katex-display')
  })

  it('returns the raw source for invalid TeX rather than throwing', () => {
    const html = renderMath('\\notacommand{', false)

    expect(html).toContain('\\notacommand')
  })
})
