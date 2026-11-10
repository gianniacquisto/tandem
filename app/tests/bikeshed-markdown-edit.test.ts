// @vitest-environment happy-dom

import { act, createElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it } from 'vitest'

import { parseDocument, serialiseDocument, type Document } from '../renderer/src/shell/blocks'
import { BikeshedBlocks } from '../renderer/src/faces/BikeshedBlocks'

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

describe('the opaque block edits in place as markdown', () => {
  it('swaps to a raw textarea, commits through the document, and returns to the block', async () => {
    const changes: Document[] = []
    host = document.createElement('div')
    document.body.append(host)
    root = createRoot(host)
    await act(async () => {
      root!.render(
        createElement(BikeshedBlocks, {
          document: parseDocument('intro\n\n| a | b |\n| - | - |'),
          focusToken: 0,
          notePath: '',
          onChange: (document) => changes.push(document),
          onSendToTodos: () => undefined,
        }),
      )
    })

    const button = Array.from(host.querySelectorAll('button')).find(
      (element) => element.textContent === 'Edit as Markdown',
    )
    expect(button).toBeTruthy()
    await act(async () => {
      button!.click()
    })

    const area = host.querySelector('textarea')
    expect(area).not.toBeNull()
    expect(area?.value).toBe('| a | b |\n| - | - |')
    expect(area?.getAttribute('aria-label')).toBe('Edit block as markdown')

    const setValue = Object.getOwnPropertyDescriptor(
      HTMLTextAreaElement.prototype,
      'value',
    )!.set!
    await act(async () => {
      setValue.call(area, 'a whole paragraph')
      area!.dispatchEvent(new Event('input', { bubbles: true }))
    })

    const done = Array.from(host.querySelectorAll('button')).find(
      (element) => element.textContent === 'Done',
    )
    expect(done).toBeTruthy()
    await act(async () => {
      done!.click()
    })

    expect(host.querySelector('textarea')).toBeNull()
    const landed = changes[changes.length - 1]
    expect(landed).toBeDefined()
    expect(serialiseDocument(landed!)).toBe('intro\n\na whole paragraph')
    expect(host.textContent).toContain('a whole paragraph')
  })

  it('cancels back to the unchanged rendered block', async () => {
    const changes: Document[] = []
    host = document.createElement('div')
    document.body.append(host)
    root = createRoot(host)
    await act(async () => {
      root!.render(
        createElement(BikeshedBlocks, {
          document: parseDocument('| a | b |\n| - | - |'),
          focusToken: 0,
          notePath: '',
          onChange: (document) => changes.push(document),
          onSendToTodos: () => undefined,
        }),
      )
    })

    await act(async () => {
      const button = Array.from(host!.querySelectorAll('button')).find(
        (element) => element.textContent === 'Edit as Markdown',
      )
      button!.click()
    })
    await act(async () => {
      const cancel = Array.from(host!.querySelectorAll('button')).find(
        (element) => element.textContent === 'Cancel',
      )
      cancel!.click()
    })

    expect(host.querySelector('textarea')).toBeNull()
    expect(changes).toHaveLength(0)
    expect(host.textContent).toContain('| a | b |')
  })
})
