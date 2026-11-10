import katex from 'katex'
import { describe, expect, it } from 'vitest'

import { renderMath, type MathLibrary } from '../renderer/src/shell/math'

/** The loader seam: the old tests injected `render`, this injects the module. */
const loaded = async (): Promise<MathLibrary> => katex

describe('the math render', () => {
  it('draws a valid inline run as KaTeX', async () => {
    const result = await renderMath('x^2', false, loaded)

    expect(result?.error).toBe(false)
    expect(result?.html).toContain('class="katex"')
    expect(result?.html).not.toContain('katex-display')
  })

  it('draws a display block in display mode', async () => {
    const result = await renderMath('x = 1', true, loaded)

    expect(result?.error).toBe(false)
    expect(result?.html).toContain('class="katex"')
    expect(result?.html).toContain('katex-display')
  })

  it('returns the raw delimiters for invalid TeX rather than throwing', async () => {
    const result = await renderMath('\\notacommand{', false, loaded)

    expect(result?.error).toBe(true)
    expect(result?.html).toContain('\\notacommand')
    expect(result?.html).toContain('$')
  })

  it('returns null when the KaTeX chunk rejects, leaving the raw source to paint', async () => {
    const failed = async (): Promise<MathLibrary> => {
      throw new Error('katex chunk failed')
    }

    await expect(renderMath('x^2', false, failed)).resolves.toBeNull()
  })
})
