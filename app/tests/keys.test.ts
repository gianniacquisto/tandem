import { describe, expect, it } from 'vitest'

import { keyIntent, type KeyInput } from '../renderer/src/shell/keys'

const key = (overrides: Partial<KeyInput>): KeyInput => ({
  key: '',
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  editable: false,
  ...overrides,
})

describe('the face keyboard', () => {
  it('focuses the column add and search on the platform modifier', () => {
    expect(keyIntent(key({ key: 'n', metaKey: true }))).toEqual({ type: 'focus-add' })
    expect(keyIntent(key({ key: 'n', ctrlKey: true }))).toEqual({ type: 'focus-add' })
    expect(keyIntent(key({ key: 'f', metaKey: true }))).toEqual({ type: 'focus-search' })
    expect(keyIntent(key({ key: 'f', ctrlKey: true }))).toEqual({ type: 'focus-search' })
  })

  it('binds nothing to the number keys now that the scope switch is gone', () => {
    expect(keyIntent(key({ key: '1', metaKey: true }))).toBeNull()
    expect(keyIntent(key({ key: '2', metaKey: true }))).toBeNull()
    expect(keyIntent(key({ key: '3', metaKey: true }))).toBeNull()
  })

  it('moves the selection with the arrows and opens with Return', () => {
    expect(keyIntent(key({ key: 'ArrowDown' }))).toEqual({ type: 'select', axis: 'row', delta: 1 })
    expect(keyIntent(key({ key: 'ArrowUp' }))).toEqual({ type: 'select', axis: 'row', delta: -1 })
    expect(keyIntent(key({ key: 'ArrowRight' }))).toEqual({
      type: 'select',
      axis: 'column',
      delta: 1,
    })
    expect(keyIntent(key({ key: 'ArrowLeft' }))).toEqual({
      type: 'select',
      axis: 'column',
      delta: -1,
    })
    expect(keyIntent(key({ key: 'Enter' }))).toEqual({ type: 'open' })
  })

  it('moves the selected card across statuses with Option, and nowhere in its lane', () => {
    expect(keyIntent(key({ key: 'ArrowRight', altKey: true }))).toEqual({
      type: 'move-across',
      delta: 1,
    })
    expect(keyIntent(key({ key: 'ArrowLeft', altKey: true }))).toEqual({
      type: 'move-across',
      delta: -1,
    })
    // The lane is a label, not an axis: Option+Up/Down binds nothing.
    expect(keyIntent(key({ key: 'ArrowDown', altKey: true }))).toBeNull()
    expect(keyIntent(key({ key: 'ArrowUp', altKey: true }))).toBeNull()
  })

  it('clears a search on Escape and dismisses a banner off a field', () => {
    expect(keyIntent(key({ key: 'Escape', editable: true }))).toEqual({ type: 'clear-search' })
    expect(keyIntent(key({ key: 'Escape' }))).toEqual({ type: 'dismiss' })
  })

  it('leaves a field alone: typing n or an arrow is the field’s, not the face’s', () => {
    for (const input of [
      key({ key: 'n', editable: true }),
      key({ key: 'ArrowDown', editable: true }),
      key({ key: 'Enter', editable: true }),
    ]) {
      expect(keyIntent(input)).toBeNull()
    }
  })

  it('still answers the platform-modified commands from inside a field', () => {
    expect(keyIntent(key({ key: 'f', metaKey: true, editable: true }))).toEqual({
      type: 'focus-search',
    })
    expect(keyIntent(key({ key: 'n', ctrlKey: true, editable: true }))).toEqual({
      type: 'focus-add',
    })
    expect(keyIntent(key({ key: '2', metaKey: true, editable: true }))).toBeNull()
  })

  it('answers nothing for a key it does not own', () => {
    expect(keyIntent(key({ key: 'x' }))).toBeNull()
    expect(keyIntent(key({ key: 'Tab' }))).toBeNull()
  })
})
