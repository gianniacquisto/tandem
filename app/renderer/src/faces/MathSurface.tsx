import { useEffect, useState } from 'react'

import { renderMath } from '../shell/math'

export type MathSurfaceProps = {
  tex: string
  display: boolean
  className?: string
  contentEditable?: boolean
  'data-rendered'?: boolean
  'data-from'?: number
}

/**
 * One piece of math drawn by the lazy KaTeX chunk.
 *
 * The note paints at once: the first frame is the raw delimiters in the
 * existing `.tandem-math-error` styling, and the typeset HTML replaces them
 * when the chunk arrives. A parse error or a failed chunk leaves the same raw
 * source visible, never a thrown render and never a placeholder.
 *
 * Display math draws a block element, so KaTeX's own `katex-display` keeps its
 * full-width layout inside the block's parchment panel; inline math draws a
 * span inside its run.
 */
export function MathSurface({ tex, display, className = '', ...rest }: MathSurfaceProps) {
  const [html, setHtml] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setHtml(null)
    void renderMath(tex, display).then((result) => {
      if (cancelled || result === null || result.error) return
      setHtml(result.html)
    })
    return () => {
      cancelled = true
    }
  }, [tex, display])

  if (html !== null) {
    return display ? (
      <div className={className} {...rest} dangerouslySetInnerHTML={{ __html: html }} />
    ) : (
      <span className={className} {...rest} dangerouslySetInnerHTML={{ __html: html }} />
    )
  }

  const delimiters = display ? '$$' : '$'
  const raw = `${delimiters}${tex}${delimiters}`
  const failed = `${className} tandem-math-error`.trim()
  return display ? (
    <div className={failed} {...rest}>
      {raw}
    </div>
  ) : (
    <span className={failed} {...rest}>
      {raw}
    </span>
  )
}
