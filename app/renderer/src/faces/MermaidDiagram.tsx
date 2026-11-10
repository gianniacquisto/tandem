import { useEffect, useState } from 'react'

import { currentTheme, renderDiagramSvg, type MermaidTheme } from '../shell/mermaid'

/**
 * One mermaid diagram, drawn by the lazy chunk.
 *
 * The note paints at once: until the SVG arrives the fence's raw source sits
 * in the existing `.tandem-mermaid-error` surface, and a failed parse or chunk
 * leaves it there rather than tearing the note down. A system scheme change
 * swaps the theme, and the effect's theme + source key makes mermaid draw the
 * mounted diagram again in the new palette rather than keeping the old colours.
 */
export function MermaidDiagram({ source }: { source: string }) {
  const [theme, setTheme] = useState<MermaidTheme>(() => currentTheme())
  const [drawn, setDrawn] = useState<{ source: string; svg: string } | null>(null)

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => setTheme(media.matches ? 'night' : 'day')
    media.addEventListener('change', onChange)
    return () => media.removeEventListener('change', onChange)
  }, [])

  useEffect(() => {
    let cancelled = false
    void renderDiagramSvg(source, theme).then((next) => {
      if (cancelled || next === null) return
      setDrawn({ source, svg: next })
    })
    return () => {
      cancelled = true
    }
  }, [source, theme])

  if (drawn !== null && drawn.source === source) {
    return (
      <div
        className="block__mermaid"
        contentEditable={false}
        dangerouslySetInnerHTML={{ __html: drawn.svg }}
      />
    )
  }

  return (
    <div className="block__mermaid tandem-mermaid-error" contentEditable={false}>
      {source}
    </div>
  )
}
