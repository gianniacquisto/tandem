import { describe, expect, it } from 'vitest'

import {
  BIKESHED_MODE_KEY,
  defaultBikeshedMode,
  editorMode,
  loadBikeshedMode,
  saveBikeshedMode,
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

describe('the preview/edit default rule', () => {
  it('opens a note with content in preview when nothing is stored', () => {
    expect(defaultBikeshedMode(null, '# Note\n\nSome words')).toBe('preview')
  })

  it('opens an empty note in edit even when preview is stored', () => {
    expect(defaultBikeshedMode('preview', '')).toBe('edit')
    expect(defaultBikeshedMode('preview', '   \n')).toBe('edit')
  })

  it('remembers the last choice for notes with content', () => {
    expect(defaultBikeshedMode('edit', '# Note')).toBe('edit')
    expect(defaultBikeshedMode('preview', '# Note')).toBe('preview')
  })

  it('falls back to preview for content when the stored value is nonsense', () => {
    expect(defaultBikeshedMode('nonsense', '# Note')).toBe('preview')
    expect(defaultBikeshedMode(null, '# Note')).toBe('preview')
  })
})

describe('the persisted preview/edit choice', () => {
  it('defaults to preview when nothing is stored', () => {
    expect(loadBikeshedMode(fakeStorage())).toBe('preview')
  })

  it('round-trips the mode the titlebar control posts', () => {
    const storage = fakeStorage()
    saveBikeshedMode('edit', storage)
    expect(storage.data.get(BIKESHED_MODE_KEY)).toBe('edit')
    expect(loadBikeshedMode(storage)).toBe('edit')
  })

  it('falls back to preview on nonsense', () => {
    expect(loadBikeshedMode(fakeStorage({ [BIKESHED_MODE_KEY]: 'nonsense' }))).toBe('preview')
  })
})

describe('the setMode page global', () => {
  it('builds the preview call the committed bundle reads', () => {
    expect(editorMode('preview')).toEqual({
      type: 'tandem-editor-call',
      name: 'setMode',
      args: ['preview'],
    })
    expect(editorMode('edit')).toEqual({
      type: 'tandem-editor-call',
      name: 'setMode',
      args: ['edit'],
    })
  })
})
