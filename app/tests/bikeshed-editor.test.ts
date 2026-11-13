// @vitest-environment happy-dom

import { act, createElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { BikeshedFace } from '../renderer/src/faces/BikeshedFace'
import type { Method } from '../renderer/src/wire/Method'

let root: Root | null = null
let host: HTMLDivElement | null = null

afterEach(() => {
  act(() => {
    root?.unmount()
  })
  host?.remove()
  root = null
  host = null
})

type Sent = { method: string; params: Record<string, unknown> }

/** A vault with one note, answered without a core. */
function fakeVault() {
  const sent: Sent[] = []
  let text = '# Index\n\nHello **world**.'
  let hash = 'hash-1'

  const bridge = {
    call: async (call: Method) => {
      sent.push({ method: call.method, params: call.params as Record<string, unknown> })
      switch (call.method) {
        case 'note.tree':
          return {
            reply: {
              nodes: [{ name: 'Index', path: 'Index.md', kind: 'note', children: [] }],
            },
          }
        case 'note.read':
          return {
            reply: {
              note: { path: 'Index.md', text, hash, modifiedAt: '2026-11-13T00:00:00Z' },
            },
          }
        case 'markdown.blocks': {
          const body = (call.params as { text?: string | null }).text ?? ''
          return {
            reply: {
              blocks: [
                { kind: 'heading', level: 1, runs: [{ text: 'Index', styles: [] }] },
                { kind: 'paragraph', runs: [{ text: body, styles: [] }] },
              ],
            },
          }
        }
        case 'note.write': {
          const next = (call.params as { text?: string | null }).text ?? ''
          text = next
          hash = 'hash-2'
          return { reply: { write: { path: 'Index.md', hash } } }
        }
        case 'note.recoveries':
          return { reply: { recoveries: [] } }
        case 'note.backlinks':
          return { reply: { path: 'Index.md', backlinks: [] } }
        case 'ticket.list':
          return { reply: { revision: 1, tickets: [] } }
        default:
          return { error: { code: 'domain', message: `not in this test: ${call.method}` } }
      }
    },
    onChanged: () => () => {},
  }

  return { bridge, sent, current: () => text }
}

function mount(bridge: unknown): Promise<void> {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  return act(async () => {
    root!.render(
      createElement(BikeshedFace, {
        projects: [],
        activeProject: null,
        onPickProject: () => {},
      }),
    )
  })
}

function button(label: string): HTMLButtonElement | undefined {
  return Array.from(host!.querySelectorAll('button')).find(
    (element) => element.textContent === label,
  )
}

function setTextarea(area: HTMLTextAreaElement, value: string): void {
  const setValue = Object.getOwnPropertyDescriptor(
    HTMLTextAreaElement.prototype,
    'value',
  )!.set!
  setValue.call(area, value)
  area.dispatchEvent(new Event('input', { bubbles: true }))
}

describe('the note face', () => {
  beforeEach(() => {
    window.localStorage.clear()
  })

  it('reads a note as the core parses it, with no textarea drawn', async () => {
    const vault = fakeVault()
    Object.defineProperty(window, 'tandem', { value: vault.bridge, configurable: true })
    await mount(vault.bridge)
    await act(async () => {
      host!.querySelector<HTMLButtonElement>('.tree__label')!.click()
    })

    expect(host!.querySelector('.tree__label')?.textContent).toContain('Index')
    expect(host!.querySelector('textarea')).toBeNull()
    expect(host!.querySelector('.note-preview')?.textContent).toContain('Index')
    expect(vault.sent.some((call) => call.method === 'markdown.blocks')).toBe(true)
  })

  it('edits the whole file in a textarea and writes it on Done', async () => {
    const vault = fakeVault()
    Object.defineProperty(window, 'tandem', { value: vault.bridge, configurable: true })
    await mount(vault.bridge)
    await act(async () => {
      host!.querySelector<HTMLButtonElement>('.tree__label')!.click()
    })
    await act(async () => {
      button('Edit')!.click()
    })

    const area = host!.querySelector('textarea')
    expect(area).not.toBeNull()
    expect(area!.value).toBe('# Index\n\nHello **world**.')
    expect(area!.getAttribute('aria-label')).toBe('Note markdown')

    await act(async () => {
      setTextarea(area!, '# Index\n\nA whole new body.')
    })
    expect(host!.querySelector('.note-preview')).toBeNull()

    await act(async () => {
      button('Done')!.click()
    })

    const written = vault.sent.filter((call) => call.method === 'note.write')
    expect(written).toHaveLength(1)
    expect(written[0]?.params).toEqual({
      path: 'Index.md',
      text: '# Index\n\nA whole new body.',
    })
    expect(vault.current()).toBe('# Index\n\nA whole new body.')
    expect(host!.querySelector('textarea')).toBeNull()
    expect(host!.textContent).toContain('A whole new body.')
  })

  it('commits on Escape, exactly as Done does', async () => {
    const vault = fakeVault()
    Object.defineProperty(window, 'tandem', { value: vault.bridge, configurable: true })
    await mount(vault.bridge)
    await act(async () => {
      host!.querySelector<HTMLButtonElement>('.tree__label')!.click()
    })
    await act(async () => {
      button('Edit')!.click()
    })
    await act(async () => {
      setTextarea(host!.querySelector('textarea')!, '# Index\n\nUnsaved words.')
      host!
        .querySelector('textarea')!
        .dispatchEvent(
          new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
        )
    })

    expect(host!.querySelector('textarea')).toBeNull()
    // Escape commits what the human typed, exactly as Done does.
    const written = vault.sent.filter((call) => call.method === 'note.write')
    expect(written).toHaveLength(1)
    expect(written[0]?.params).toMatchObject({ text: '# Index\n\nUnsaved words.' })
  })

  it('asks for no health report and offers no hidden layer', async () => {
    const vault = fakeVault()
    Object.defineProperty(window, 'tandem', { value: vault.bridge, configurable: true })
    await mount(vault.bridge)

    expect(vault.sent.some((call) => call.method === 'note.health')).toBe(false)
    expect(host!.textContent).not.toContain('Show hidden')
    expect(host!.textContent).not.toContain("Today's note")
    expect(host!.textContent).not.toContain('Health')
  })
})
