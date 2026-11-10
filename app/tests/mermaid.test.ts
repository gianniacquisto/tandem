import { describe, expect, it } from 'vitest'

import { renderDiagram, themeVariables } from '../renderer/src/shell/mermaid'

function fakeElement() {
  const classes = new Set<string>()
  return {
    classList: {
      add: (name: string) => classes.add(name),
      remove: (name: string) => classes.delete(name),
      contains: (name: string) => classes.has(name),
    },
    textContent: '',
    innerHTML: '',
    classes,
  }
}

describe('the mermaid render', () => {
  it('mounts the rendered SVG into the element', async () => {
    const element = fakeElement()
    let calls = 0

    const svg = await renderDiagram(element, 'graph TD; A-->B', 'day', {
      render: async () => {
        calls += 1
        return '<svg>diagram</svg>'
      },
      cache: new Map(),
    })

    expect(svg).toBe('<svg>diagram</svg>')
    expect(element.innerHTML).toBe('<svg>diagram</svg>')
    expect(element.classList.contains('tandem-mermaid-error')).toBe(false)
    expect(calls).toBe(1)
  })

  it('leaves the raw source under the error class when the render rejects', async () => {
    const element = fakeElement()

    const result = await renderDiagram(element, 'graph TD; A--', 'day', {
      render: async () => {
        throw new Error('parse error')
      },
      cache: new Map(),
    })

    expect(result).toBeNull()
    expect(element.textContent).toBe('graph TD; A--')
    expect(element.classList.contains('tandem-mermaid-error')).toBe(true)
  })

  it('reuses the cache for the same theme and source, and rerenders on a theme change', async () => {
    const element = fakeElement()
    const cache = new Map<string, string>()
    let calls = 0
    const render = async (source: string, theme: string) => {
      calls += 1
      return `<svg data-theme="${theme}">${source}</svg>`
    }

    await renderDiagram(element, 'graph TD; A-->B', 'day', { render, cache })
    await renderDiagram(element, 'graph TD; A-->B', 'day', { render, cache })
    expect(calls).toBe(1)
    expect(element.innerHTML).toContain('data-theme="day"')

    await renderDiagram(element, 'graph TD; A-->B', 'night', { render, cache })
    expect(calls).toBe(2)
    expect(element.innerHTML).toContain('data-theme="night"')
  })

  it('maps the app tokens, with the day values as fallbacks', () => {
    const variables = themeVariables({ documentElement: {} } as unknown as Document)

    expect(variables.background).toBe('#f1e9d7')
    expect(variables.primaryColor).toBe('#e7deca')
    expect(variables.primaryTextColor).toBe('#211d18')
    expect(variables.lineColor).toBe('#877d6b')
    expect(variables.noteBorderColor).toBe('#a9745c')
  })
})
