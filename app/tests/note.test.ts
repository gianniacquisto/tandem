import { describe, expect, it } from 'vitest'

import type { Call } from '../src/shared/bridge'
import {
  applyDirty,
  arrival,
  conflictStillApplies,
  defaultCreateFolder,
  keepMine,
  openBuffer,
  resolveFolderPath,
  resolveNotePath,
  saveNote,
  stashDraft,
  takeDraft,
  takeTheirs,
  type NoteBuffer,
  type NoteCall,
} from '../renderer/src/shell/note'

type Answer = { reply: unknown } | { error: { code: string; message: string } }

/** A call that answers a scripted sequence, and records every method it saw. */
function scripted(answers: Answer[]): { call: NoteCall; seen: Call[] } {
  const seen: Call[] = []
  const call = async (request: Call): Promise<Answer> => {
    seen.push(request)
    const answer = answers.shift()
    if (answer === undefined) throw new Error(`unscripted call: ${request.method}`)
    return answer
  }
  return { call: call as unknown as NoteCall, seen }
}

function read(path: string, text: string, hash: string) {
  return { reply: { note: { path, text, hash, modifiedAt: '2026-10-30T12:00:00Z' } } }
}

function write(path: string, hash: string) {
  return { reply: { write: { path, hash } } }
}

const buffer: NoteBuffer = {
  path: 'Index.md',
  text: 'mine',
  baseline: 'aaaa',
  dirty: true,
}

describe('the note save discipline', () => {
  it('writes the buffer when the disk still holds the hash it read', async () => {
    const { call, seen } = scripted([read('Index.md', 'theirs', 'aaaa'), write('Index.md', 'bbbb')])

    const outcome = await saveNote(call, buffer)

    expect(outcome).toEqual({ type: 'saved', hash: 'bbbb' })
    expect(seen.map((entry) => entry.method)).toEqual(['note.read', 'note.write'])
    expect(seen[1]).toEqual({
      method: 'note.write',
      params: { path: 'Index.md', text: 'mine' },
    })
  })

  it('withholds the write and carries both bodies when the disk moved', async () => {
    const { call, seen } = scripted([read('Index.md', 'their words', 'cccc')])

    const outcome = await saveNote(call, buffer)

    expect(outcome).toEqual({
      type: 'conflict',
      theirs: { text: 'their words', hash: 'cccc' },
    })
    expect(seen.map((entry) => entry.method)).toEqual(['note.read'])
  })

  it('answers the core refusal as an error, never a throw', async () => {
    const { call } = scripted([
      { error: { code: 'domain', message: 'the note "Index.md" is unknown' } },
    ])

    const outcome = await saveNote(call, buffer)

    expect(outcome).toEqual({ type: 'error', message: 'the note "Index.md" is unknown' })
  })

  it('does not read or write a clean buffer', async () => {
    const { call, seen } = scripted([])

    const outcome = await saveNote(call, { ...buffer, dirty: false })

    expect(outcome).toEqual({ type: 'saved', hash: 'aaaa' })
    expect(seen).toEqual([])
  })
})

describe('the conflict choices', () => {
  const theirs = { text: 'their words', hash: 'cccc' }

  it('keeps mine by moving the baseline to the disk it is about to replace', () => {
    const mine = keepMine(buffer, theirs)

    expect(mine).toEqual({ ...buffer, baseline: 'cccc', dirty: true })
  })

  it('takes theirs by adopting the on-disk body and clearing the dirty bit', () => {
    const adopted = takeTheirs(buffer, theirs)

    expect(adopted).toEqual({ path: 'Index.md', text: 'their words', baseline: 'cccc', dirty: false })
  })
})

describe('an external change arriving at an open note', () => {
  it('is a conflict while the buffer is dirty', () => {
    expect(arrival(buffer, { text: 'theirs', hash: 'cccc' })).toEqual({
      type: 'conflict',
      theirs: { text: 'theirs', hash: 'cccc' },
    })
  })

  it('is adopted silently while the buffer is clean', () => {
    expect(arrival({ ...buffer, dirty: false }, { text: 'theirs', hash: 'cccc' })).toEqual({
      type: 'adopted',
      buffer: { path: 'Index.md', text: 'theirs', baseline: 'cccc', dirty: false },
    })
  })

  it('is nothing when the hash still agrees', () => {
    expect(arrival(buffer, { text: 'mine', hash: 'aaaa' })).toEqual({ type: 'unchanged' })
  })
})

describe('an opened note', () => {
  it('starts clean at the hash the read reported', () => {
    expect(openBuffer({ path: 'Index.md', text: '# Index', hash: 'aaaa' })).toEqual({
      path: 'Index.md',
      text: '# Index',
      baseline: 'aaaa',
      dirty: false,
    })
  })
})

describe('a draft whose save did not land', () => {
  it('keeps a dirty buffer and hands it back once', () => {
    stashDraft({ ...buffer, path: 'draft.md', text: 'unsaved' })
    expect(takeDraft()).toEqual({ ...buffer, path: 'draft.md', text: 'unsaved' })
    expect(takeDraft()).toBeNull()
  })

  it('does not keep a clean buffer', () => {
    stashDraft({ ...buffer, path: 'clean.md', dirty: false })
    expect(takeDraft()).toBeNull()
  })
})

describe('a stale save completing after Take theirs', () => {
  const saved: NoteBuffer = { path: 'Index.md', text: 'mine', baseline: 'aaaa', dirty: true }
  const theirs = { text: 'their words', hash: 'cccc' }

  it('ignores the conflict once the buffer adopted the on-disk body', () => {
    const adopted = takeTheirs(saved, theirs)

    expect(conflictStillApplies(adopted, saved)).toBe(false)
  })

  it('keeps the conflict while the buffer still holds the old baseline', () => {
    const typing = { ...saved, text: 'mine plus more typing' }

    expect(conflictStillApplies(typing, saved)).toBe(true)
  })

  it('ignores the conflict after the note was replaced', () => {
    const other: NoteBuffer = { path: 'Other.md', text: 'mine', baseline: 'aaaa', dirty: true }

    expect(conflictStillApplies(other, saved)).toBe(false)
    expect(conflictStillApplies(null, saved)).toBe(false)
  })
})

describe('an editor message that changes nothing', () => {
  it('leaves a clean buffer clean when the text is identical', () => {
    const clean: NoteBuffer = { ...buffer, dirty: false }

    expect(applyDirty(clean, clean.text)).toBe(clean)
  })

  it('keeps the baseline when new typing dirties the buffer', () => {
    const clean: NoteBuffer = { ...buffer, dirty: false, baseline: 'aaaa', text: 'old' }

    expect(applyDirty(clean, 'old plus typing')).toEqual({
      ...clean,
      text: 'old plus typing',
      dirty: true,
    })
  })
})

describe('the creation default location', () => {
  it('is the open note folder, else the vault root', () => {
    expect(defaultCreateFolder('projects/fixture/fixture.md')).toBe('projects/fixture')
    expect(defaultCreateFolder('Index.md')).toBe('')
    expect(defaultCreateFolder(null)).toBe('')
  })

  it('joins a prompted note name under the default folder, adding .md once', () => {
    expect(resolveNotePath('', 'hello')).toBe('hello.md')
    expect(resolveNotePath('', 'hello.md')).toBe('hello.md')
    expect(resolveNotePath('projects/fixture', 'note')).toBe('projects/fixture/note.md')
    expect(resolveNotePath('projects/fixture', '  spaced  ')).toBe('projects/fixture/spaced.md')
  })

  it('joins a prompted folder name under the default folder', () => {
    expect(resolveFolderPath('', 'daily')).toBe('daily')
    expect(resolveFolderPath('projects', 'sub')).toBe('projects/sub')
    expect(resolveFolderPath('projects', '  sub  ')).toBe('projects/sub')
  })
})
