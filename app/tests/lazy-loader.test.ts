// @vitest-environment happy-dom
/// <reference lib="dom" />
// The mount pulls the block surface into this (node) program, so it reads the
// renderer's window surface declaration too.
/// <reference path="../renderer/src/env.d.ts" />
import { act, createElement, type ReactElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { BikeshedBlocks } from '../renderer/src/faces/BikeshedBlocks'
import { MermaidDiagram } from '../renderer/src/faces/MermaidDiagram'
import { parseDocument } from '../renderer/src/shell/blocks'
import { diagramCache } from '../renderer/src/shell/mermaid'
import type { MathLibrary, MermaidLibrary } from '../renderer/src/shell/lazy'

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

/**
 * The chunk loaders, replaced at the module seam every component imports, so a
 * mounted note proves what it asks for: the loader's own memoization is proved
 * against the real module with an injected import below.
 */
const spies = vi.hoisted(() => ({
  loadMermaid: vi.fn(),
  loadMath: vi.fn(),
}))

vi.mock('../renderer/src/shell/lazy', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../renderer/src/shell/lazy')>()
  return { ...actual, loadMermaid: spies.loadMermaid, loadMath: spies.loadMath }
})

type FakeMermaid = {
  initialize: ReturnType<typeof vi.fn>
  render: ReturnType<typeof vi.fn>
}

function fakeMermaid(): { library: MermaidLibrary; calls: FakeMermaid } {
  const calls: FakeMermaid = {
    initialize: vi.fn(),
    render: vi.fn(async (_id: string, source: string) => ({ svg: `<svg>${source}</svg>` })),
  }
  return { library: calls as unknown as MermaidLibrary, calls }
}

function fakeMath(): { library: MathLibrary; render: ReturnType<typeof vi.fn> } {
  const render = vi.fn((tex: string) => `<span class="katex">${tex}</span>`)
  return { library: { renderToString: render } as unknown as MathLibrary, render }
}

/** A fresh copy of the real loader module, so its memo cannot leak across tests. */
async function realLoader() {
  vi.resetModules()
  return vi.importActual<typeof import('../renderer/src/shell/lazy')>(
    '../renderer/src/shell/lazy',
  )
}

describe('the loader imports once', () => {
  it('loads mermaid once and initializes it once per theme', async () => {
    const { loadMermaid } = await realLoader()
    const { library, calls } = fakeMermaid()
    const load = vi.fn(async () => library)

    await loadMermaid('day', load)
    await loadMermaid('day', load)

    expect(load).toHaveBeenCalledTimes(1)
    expect(calls.initialize).toHaveBeenCalledTimes(1)
    expect(calls.initialize).toHaveBeenCalledWith(
      expect.objectContaining({ securityLevel: 'strict', theme: 'base' }),
    )

    await loadMermaid('night', load)
    expect(load).toHaveBeenCalledTimes(1)
    expect(calls.initialize).toHaveBeenCalledTimes(2)
  })

  it('loads katex once for every math render', async () => {
    const { loadMath } = await realLoader()
    const { library } = fakeMath()
    const load = vi.fn(async () => library)

    await loadMath(load)
    await loadMath(load)

    expect(load).toHaveBeenCalledTimes(1)
  })
})

describe('a rejected chunk', () => {
  it('clears the mermaid memo so the next render retries', async () => {
    const { loadMermaid } = await realLoader()
    const { library } = fakeMermaid()
    const load = vi
      .fn()
      .mockRejectedValueOnce(new Error('mermaid chunk failed'))
      .mockResolvedValueOnce(library)

    await expect(loadMermaid('day', load)).rejects.toThrow('mermaid chunk failed')
    await expect(loadMermaid('day', load)).resolves.toBe(library)
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('clears the katex memo so the next render retries', async () => {
    const { loadMath } = await realLoader()
    const { library } = fakeMath()
    const load = vi
      .fn()
      .mockRejectedValueOnce(new Error('katex chunk failed'))
      .mockResolvedValueOnce(library)

    await expect(loadMath(load)).rejects.toThrow('katex chunk failed')
    await expect(loadMath(load)).resolves.toBe(library)
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('rejects the math render and lets the next render retry', async () => {
    const { renderMath } = await vi.importActual<typeof import('../renderer/src/shell/math')>(
      '../renderer/src/shell/math',
    )
    const { library } = fakeMath()
    const load = vi
      .fn()
      .mockRejectedValueOnce(new Error('katex chunk failed'))
      .mockResolvedValueOnce(library)

    await expect(renderMath('x^2', false, load)).resolves.toBeNull()
    await expect(renderMath('x^2', false, load)).resolves.toMatchObject({
      error: false,
      html: expect.stringContaining('class="katex"'),
    })
    expect(load).toHaveBeenCalledTimes(2)
  })
})

describe('the parsed content is the only trigger', () => {
  let root: Root | null = null
  let host: HTMLDivElement

  beforeEach(() => {
    diagramCache.clear()
    spies.loadMermaid.mockReset().mockResolvedValue(fakeMermaid().library)
    spies.loadMath.mockReset().mockResolvedValue(fakeMath().library)
    host = document.createElement('div')
    document.body.append(host)
  })

  afterEach(() => {
    act(() => {
      root?.unmount()
      root = null
    })
    host.remove()
  })

  async function mount(element: ReactElement): Promise<void> {
    root = createRoot(host)
    await act(async () => {
      root?.render(element)
    })
  }

  function note(source: string): ReactElement {
    return createElement(BikeshedBlocks, {
      document: parseDocument(source),
      focusToken: 0,
      notePath: 'note.md',
      onChange: () => undefined,
      onSendToTodos: () => undefined,
      noteIndex: [],
    })
  }

  it('asks for neither library when the note has no fence and no math', async () => {
    await mount(note('# Plain\n\nOnly words here.\n'))

    expect(spies.loadMermaid).not.toHaveBeenCalled()
    expect(spies.loadMath).not.toHaveBeenCalled()
  })

  it('asks for mermaid alone when the note has a fence', async () => {
    await mount(note('# Render\n\n```mermaid\ngraph TD; A-->B\n```\n'))

    expect(spies.loadMermaid).toHaveBeenCalledTimes(1)
    expect(spies.loadMath).not.toHaveBeenCalled()
  })

  it('asks for math alone when the note has an inline math run', async () => {
    await mount(note('# Render\n\nA $x^2$ run.\n'))

    expect(spies.loadMath).toHaveBeenCalledTimes(1)
    expect(spies.loadMermaid).not.toHaveBeenCalled()
  })

  it('asks for math alone when the note has a display math block', async () => {
    await mount(note('# Render\n\n$$\nx = 1\n$$\n'))

    expect(spies.loadMath).toHaveBeenCalledTimes(1)
    expect(spies.loadMermaid).not.toHaveBeenCalled()
  })

  it('keeps the raw math delimiters when the chunk rejects', async () => {
    spies.loadMath.mockRejectedValue(new Error('katex chunk failed'))
    await mount(note('# Render\n\n$$\nx = 1\n$$\n'))

    expect(host.innerHTML).toContain('x = 1')
    expect(host.innerHTML).toContain('$')
    expect(host.innerHTML).toContain('tandem-math-error')
    expect(host.innerHTML).not.toContain('class="katex"')
  })
})

describe('a theme flip', () => {
  let root: Root | null = null
  let host: HTMLDivElement
  let listeners: Array<() => void>
  let matches: boolean

  beforeEach(() => {
    diagramCache.clear()
    matches = false
    listeners = []
    window.matchMedia = ((query: string) => ({
      get matches() {
        return matches
      },
      media: query,
      onchange: null,
      addEventListener: (_event: string, listener: () => void) => listeners.push(listener),
      removeEventListener: (_event: string, listener: () => void) => {
        listeners = listeners.filter((entry) => entry !== listener)
      },
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia
    host = document.createElement('div')
    document.body.append(host)
  })

  afterEach(() => {
    act(() => {
      root?.unmount()
      root = null
    })
    host.remove()
  })

  it('draws the diagram again in the new palette', async () => {
    const { library, calls } = fakeMermaid()
    spies.loadMermaid.mockReset().mockResolvedValue(library)

    root = createRoot(host)
    await act(async () => {
      root?.render(createElement(MermaidDiagram, { source: 'graph TD; A-->B' }))
    })
    expect(calls.render).toHaveBeenCalledTimes(1)

    matches = true
    await act(async () => {
      for (const listener of listeners) listener()
    })

    expect(calls.render).toHaveBeenCalledTimes(2)
  })
})
