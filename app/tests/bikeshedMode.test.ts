import { describe, expect, it } from 'vitest'

import {
  BIKESHED_MODE_KEY,
  loadMode,
  openMode,
  saveMode,
  switchAfterFlush,
  switchIntent,
} from '../renderer/src/shell/modes'

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

describe('the Write default rule', () => {
  it('opens a note with content in Write when nothing is stored', () => {
    expect(openMode(null, '# Note\n\nSome words')).toBe('write')
  })

  it('opens an empty or whitespace-only note in Write even when Source is stored', () => {
    expect(openMode('source', '')).toBe('write')
    expect(openMode('source', '   \n')).toBe('write')
    expect(openMode('write', '')).toBe('write')
  })

  it('remembers the last choice for notes with content, including Source', () => {
    expect(openMode('source', '# Note')).toBe('source')
    expect(openMode('write', '# Note')).toBe('write')
  })

  it('reads a legacy preview or edit, and nonsense, as Write', () => {
    expect(openMode('preview', '# Note')).toBe('write')
    expect(openMode('edit', '# Note')).toBe('write')
    expect(openMode('nonsense', '# Note')).toBe('write')
    expect(openMode(null, '# Note')).toBe('write')
  })
})

describe('the persisted mode choice', () => {
  it('defaults to Write when nothing is stored', () => {
    expect(loadMode(fakeStorage())).toBe('write')
  })

  it('round-trips Source through the key', () => {
    const storage = fakeStorage()
    saveMode('source', storage)
    expect(storage.data.get(BIKESHED_MODE_KEY)).toBe('source')
    expect(loadMode(storage)).toBe('source')

    saveMode('write', storage)
    expect(storage.data.get(BIKESHED_MODE_KEY)).toBe('write')
    expect(loadMode(storage)).toBe('write')
  })

  it('reads a stored preview or edit as Write', () => {
    expect(loadMode(fakeStorage({ [BIKESHED_MODE_KEY]: 'preview' }))).toBe('write')
    expect(loadMode(fakeStorage({ [BIKESHED_MODE_KEY]: 'edit' }))).toBe('write')
  })

  it('falls back to Write on nonsense', () => {
    expect(loadMode(fakeStorage({ [BIKESHED_MODE_KEY]: 'nonsense' }))).toBe('write')
  })
})

describe('the switch intent', () => {
  it('is blocked while a conflict is pending', () => {
    expect(switchIntent({ dirty: false, saving: false, conflict: true })).toBe('blocked')
    expect(switchIntent({ dirty: true, saving: true, conflict: true })).toBe('blocked')
  })

  it('waits while a save is in flight', () => {
    expect(switchIntent({ dirty: false, saving: true, conflict: false })).toBe('wait')
    expect(switchIntent({ dirty: true, saving: true, conflict: false })).toBe('wait')
  })

  it('flushes a dirty buffer first', () => {
    expect(switchIntent({ dirty: true, saving: false, conflict: false })).toBe('flush')
  })

  it('switches a clean buffer without a write', () => {
    expect(switchIntent({ dirty: false, saving: false, conflict: false })).toBe('switch')
  })
})

describe('a switch after a flush', () => {
  it('switches only when the save landed', () => {
    expect(switchAfterFlush('saved')).toBe('switch')
    expect(switchAfterFlush('conflict')).toBe('stay')
    expect(switchAfterFlush('error')).toBe('stay')
  })
})
