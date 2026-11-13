// @vitest-environment happy-dom

import { act, createElement } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it } from 'vitest'

import { parseDocument, serialiseDocument, type Document } from '../renderer/src/shell/blocks'
import { fromDocument, rowCount, rowText } from '../renderer/src/shell/editor-ops'
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

async function mount(markdown: string, changes: Document[]): Promise<HTMLDivElement> {
  host = document.createElement('div')
  document.body.append(host)
  const target = host
  root = createRoot(target)
  await act(async () => {
    root!.render(
      createElement(BikeshedBlocks, {
        document: parseDocument(markdown),
        focusToken: 0,
        onChange: (document) => changes.push(document),
      }),
    )
  })
  return target
}

function caretAt(node: Node, offset: number): void {
  const range = document.createRange()
  range.setStart(node, offset)
  range.collapse(true)
  const selection = window.getSelection()
  expect(selection).not.toBeNull()
  selection!.removeAllRanges()
  selection!.addRange(range)
}

/** Type a character the way the browser does: the text lands, then `input` fires. */
async function type(host: HTMLElement, node: Node, text: string): Promise<void> {
  node.textContent = text
  caretAt(node, text.length)
  await act(async () => {
    host.querySelector('.block__row')!.dispatchEvent(new Event('input', { bubbles: true }))
  })
  // A browser leaves the caret after the character it just typed; the
  // harness's queued selectionchange can move it back, so restore it.
  caretAt(node, text.length)
}

async function pressEnter(host: HTMLElement): Promise<void> {
  await act(async () => {
    host
      .querySelector('.blocks')!
      .dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  })
}

/** The rows of the first block as the model holds them. */
function modelRows(document: Document): string[] {
  const state = fromDocument(document)
  const surface = state.blocks[0]!
  return Array.from({ length: rowCount(surface) }, (_, row) => rowText(surface, row))
}

describe('typing at a list row boundary', () => {
  it('a character typed at the row boundary reaches the model', async () => {
    const changes: Document[] = []
    const target = await mount('- one', changes)
    const row = target.querySelector('.block__row')!
    const stray = document.createTextNode('')
    row.insertBefore(stray, row.firstChild)

    await type(target, stray, 'X')

    const landed = changes[changes.length - 1]
    expect(landed).toBeDefined()
    expect(modelRows(landed!)).toEqual(['Xone'])
  })

  it('Enter after boundary typing adds an item instead of removing one', async () => {
    const changes: Document[] = []
    const target = await mount('- one', changes)
    const row = target.querySelector('.block__row')!
    const stray = document.createTextNode('')
    row.insertBefore(stray, row.firstChild)

    await type(target, stray, 'X')
    await pressEnter(target)

    const landed = changes[changes.length - 1]
    expect(landed).toBeDefined()
    const state = fromDocument(landed!)
    expect(state.blocks[0]?.kind).toBe('list')
    expect(modelRows(landed!)).toHaveLength(2)
    expect(modelRows(landed!).join('')).toContain('Xone')
  })

  it('a checked task row never feeds its check to the model', async () => {
    const changes: Document[] = []
    const target = await mount('- [x] done', changes)
    const text = target.querySelector('.block__row [data-text]')!
    const node = text.firstChild
    expect(node).not.toBeNull()

    await type(target, node!, 'dones')

    const landed = changes[changes.length - 1]
    expect(landed).toBeDefined()
    const [item] = modelRows(landed!)
    expect(item).toBe('dones')
    expect(item).not.toContain('\u2713')
    expect(item).not.toContain('[x]')
  })

  it('no stray row-level text node survives a render', async () => {
    const changes: Document[] = []
    const target = await mount('- one', changes)
    const row = target.querySelector('.block__row')!
    const stray = document.createTextNode('')
    row.insertBefore(stray, row.firstChild)

    await type(target, stray, 'X')
    await pressEnter(target)

    for (const element of Array.from(target.querySelectorAll('.block__row'))) {
      const textNodes = Array.from(element.childNodes).filter(
        (child) => child.nodeType === Node.TEXT_NODE,
      )
      expect(textNodes).toEqual([])
      expect(element.querySelector('[data-text]')).not.toBeNull()
    }
  })

  it('two Enters at the end of a list leave the caret in a paragraph below it', async () => {
    const changes: Document[] = []
    const target = await mount('- one', changes)
    const text = target.querySelector('.block__row [data-text]')!
    caretAt(text.firstChild!, 3)

    await pressEnter(target)
    await pressEnter(target)

    const blocks = Array.from(target.querySelectorAll('.blocks > [data-block]')) as HTMLElement[]
    expect(blocks).toHaveLength(2)
    expect(blocks[0]!.querySelector('.block')!.className).toContain('block--bullet')
    expect(blocks[0]!.querySelectorAll('.block__row')).toHaveLength(1)
    expect(blocks[1]!.querySelector('.block')!.className).toContain('block--paragraph')

    const selection = window.getSelection()!
    const node = selection.anchorNode!
    const anchor = node.nodeType === Node.TEXT_NODE ? node.parentElement : (node as HTMLElement)
    expect(anchor!.closest('[data-block]')).toBe(blocks[1])
    expect(selection.anchorOffset).toBe(0)

    const landed = changes[changes.length - 1]
    expect(landed).toBeDefined()
    expect(serialiseDocument(landed!)).toBe('- one\n')
  })
})

describe('a character at the row boundary', () => {
  it('splits where the caret sits at the start of the row', async () => {
    const changes: Document[] = []
    const target = await mount('- one', changes)
    const row = target.querySelector('.block__row')!
    const stray = document.createTextNode('')
    row.insertBefore(stray, row.firstChild)

    await type(target, stray, 'X')
    await pressEnter(target)

    const landed = changes[changes.length - 1]
    expect(landed).toBeDefined()
    expect(modelRows(landed!)).toEqual(['X', 'one'])
  })

  it('splits where the caret sits at the end of the row', async () => {
    const changes: Document[] = []
    const target = await mount('- one', changes)
    const row = target.querySelector('.block__row')!
    const stray = document.createTextNode('')
    row.append(stray)

    await type(target, stray, 'X')
    await pressEnter(target)

    const landed = changes[changes.length - 1]
    expect(landed).toBeDefined()
    expect(modelRows(landed!)).toEqual(['oneX', ''])
  })
})

describe('the list marker', () => {
  it('is a row attribute, not a rendered span', async () => {
    const changes: Document[] = []
    const target = await mount('- one\n\n1. a', changes)

    expect(target.querySelector('.block__marker')).toBeNull()
    const bullet = target.querySelector('.block--bullet .block__row')!
    expect(bullet.getAttribute('data-marker')).toBe('- ')
    const ordered = target.querySelector('.block--ordered .block__row')!
    expect(ordered.getAttribute('data-marker')).toBe('1. ')
  })
})
