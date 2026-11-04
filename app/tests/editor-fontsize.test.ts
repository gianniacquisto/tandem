import { describe, expect, it } from 'vitest'

import {
  EDITOR_FONT_SIZE_DEFAULT,
  EDITOR_FONT_SIZE_MAX,
  EDITOR_FONT_SIZE_MIN,
  clampEditorFontSize,
  loadEditorFontSize,
  saveEditorFontSize,
  EDITOR_FONT_SIZE_KEY,
} from '../renderer/src/shell/editor'

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

  it('round-trips a size the toolbar posts', () => {
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

  it('clamps the value it saves, so a stray post cannot stick an absurd size', () => {
    const storage = fakeStorage()
    saveEditorFontSize(99, storage)
    expect(loadEditorFontSize(storage)).toBe(EDITOR_FONT_SIZE_MAX)
    expect(clampEditorFontSize('nope')).toBe(EDITOR_FONT_SIZE_DEFAULT)
  })
})
