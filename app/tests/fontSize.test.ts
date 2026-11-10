import { describe, expect, it } from 'vitest'

import {
  EDITOR_FONT_SIZE_DEFAULT,
  EDITOR_FONT_SIZE_MAX,
  EDITOR_FONT_SIZE_MIN,
  clampEditorFontSize,
  loadEditorFontSize,
  saveEditorFontSize,
  stepEditorFontSize,
  EDITOR_FONT_SIZE_KEY,
} from '../renderer/src/shell/fontSize'

function fakeStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial))
  return {
    getItem: (key: string) => (data.has(key) ? data.get(key)! : null),
    setItem: (key: string, value: string) => {
      data.set(key, value)
    },
    data,
  }
}

describe('the persisted note font size', () => {
  it('defaults to the 16px reading size when nothing is stored', () => {
    expect(loadEditorFontSize(fakeStorage())).toBe(EDITOR_FONT_SIZE_DEFAULT)
    expect(EDITOR_FONT_SIZE_DEFAULT).toBe(16)
  })

  it('round-trips a size the control will post', () => {
    const storage = fakeStorage()
    saveEditorFontSize(18, storage)
    expect(storage.data.get(EDITOR_FONT_SIZE_KEY)).toBe('18')
    expect(loadEditorFontSize(storage)).toBe(18)
  })

  it('clamps a stored size into the sane range and falls back on nonsense', () => {
    expect(loadEditorFontSize(fakeStorage({ [EDITOR_FONT_SIZE_KEY]: '99' }))).toBe(
      EDITOR_FONT_SIZE_MAX,
    )
    expect(loadEditorFontSize(fakeStorage({ [EDITOR_FONT_SIZE_KEY]: '9' }))).toBe(
      EDITOR_FONT_SIZE_MIN,
    )
    expect(loadEditorFontSize(fakeStorage({ [EDITOR_FONT_SIZE_KEY]: 'nope' }))).toBe(
      EDITOR_FONT_SIZE_DEFAULT,
    )
  })

  it('clamps the value it saves, so a stray write cannot stick an absurd size', () => {
    const storage = fakeStorage()
    saveEditorFontSize(99, storage)
    expect(loadEditorFontSize(storage)).toBe(EDITOR_FONT_SIZE_MAX)
    expect(clampEditorFontSize('nope')).toBe(EDITOR_FONT_SIZE_DEFAULT)
  })

  it('steps one pixel up and down from the stored size', () => {
    expect(stepEditorFontSize(16, 1)).toBe(17)
    expect(stepEditorFontSize(16, -1)).toBe(15)
  })

  it('holds at each bound rather than running past it', () => {
    expect(stepEditorFontSize(EDITOR_FONT_SIZE_MAX, 1)).toBe(EDITOR_FONT_SIZE_MAX)
    expect(stepEditorFontSize(EDITOR_FONT_SIZE_MIN, -1)).toBe(EDITOR_FONT_SIZE_MIN)
  })

  it('reads a nonsense current as the default, then steps from it', () => {
    expect(stepEditorFontSize('nope', 1)).toBe(EDITOR_FONT_SIZE_DEFAULT + 1)
    expect(stepEditorFontSize(undefined, -1)).toBe(EDITOR_FONT_SIZE_DEFAULT - 1)
  })

  it('round-trips a stepped value through the stored key', () => {
    const storage = fakeStorage()
    saveEditorFontSize(stepEditorFontSize(20, 1), storage)
    expect(loadEditorFontSize(storage)).toBe(EDITOR_FONT_SIZE_MAX)
    saveEditorFontSize(stepEditorFontSize(16, 1), storage)
    expect(loadEditorFontSize(storage)).toBe(17)
  })
})
