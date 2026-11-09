import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import type { Call } from '../src/shared/bridge'
import { documentBlocks, editBlock, parseDocument } from '../renderer/src/shell/blocks'
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

const here = path.dirname(fileURLToPath(import.meta.url))
const corpus = path.join(here, 'fixtures', 'bikeshed-corpus')
const manifest = JSON.parse(readFileSync(path.join(corpus, 'manifest.json'), 'utf8')) as {
  files: { name: string }[]
}
const corpusText = (name: string): string => readFileSync(path.join(corpus, name), 'utf8')

const buffer: NoteBuffer = {
  path: 'Index.md',
  text: 'mine',
  baseline: 'aaaa',
  dirty: true,
  document: parseDocument('mine'),
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

    expect(adopted).toEqual({
      path: 'Index.md',
      text: 'their words',
      baseline: 'cccc',
      dirty: false,
      document: parseDocument('their words'),
    })
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
      buffer: {
        path: 'Index.md',
        text: 'theirs',
        baseline: 'cccc',
        dirty: false,
        document: parseDocument('theirs'),
      },
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
      document: parseDocument('# Index'),
    })
  })
})

describe('a draft whose save did not land', () => {
  it('keeps a dirty buffer and hands it back once', () => {
    stashDraft({ ...buffer, path: 'draft.md', text: 'unsaved', document: parseDocument('unsaved') })
    expect(takeDraft()).toEqual({
      ...buffer,
      path: 'draft.md',
      text: 'unsaved',
      document: parseDocument('unsaved'),
    })
    expect(takeDraft()).toBeNull()
  })

  it('does not keep a clean buffer', () => {
    stashDraft({ ...buffer, path: 'clean.md', dirty: false })
    expect(takeDraft()).toBeNull()
  })
})

describe('a stale save completing after Take theirs', () => {
  const saved: NoteBuffer = {
    path: 'Index.md',
    text: 'mine',
    baseline: 'aaaa',
    dirty: true,
    document: parseDocument('mine'),
  }
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
    const other: NoteBuffer = {
    path: 'Other.md',
    text: 'mine',
    baseline: 'aaaa',
    dirty: true,
    document: parseDocument('mine'),
  }

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
      document: parseDocument('old plus typing'),
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

describe('the document behind the buffer', () => {
  it('a dirty buffer opened from a corpus note writes exactly the original bytes', async () => {
    for (const entry of manifest.files) {
      const text = corpusText(entry.name)
      const opened: NoteBuffer = {
        ...openBuffer({ path: entry.name, text, hash: 'aaaa' }),
        dirty: true,
      }
      const { call, seen } = scripted([
        read(entry.name, text, 'aaaa'),
        write(entry.name, 'bbbb'),
      ])

      const outcome = await saveNote(call, opened)

      expect(outcome, entry.name).toEqual({ type: 'saved', hash: 'bbbb' })
      expect(seen[1], entry.name).toEqual({
        method: 'note.write',
        params: { path: entry.name, text },
      })
    }
  })

  it('a one-block edit writes the original with only that block changed', async () => {
    const original = '# Title\n\nThe middle paragraph.\n\nAfter.\n'
    const document = parseDocument(original)
    const paragraph = documentBlocks(document)[1]
    if (paragraph === undefined || paragraph.kind !== 'paragraph') throw new Error('no paragraph')

    const edited = editBlock(document, 1, { ...paragraph, text: 'A replacement sentence.' })
    const { call, seen } = scripted([read('note.md', original, 'aaaa'), write('note.md', 'bbbb')])
    const outcome = await saveNote(call, {
      path: 'note.md',
      text: original,
      baseline: 'aaaa',
      dirty: true,
      document: edited,
    })

    expect(outcome).toEqual({ type: 'saved', hash: 'bbbb' })
    const written = seen[1]
    if (written === undefined || written.method !== 'note.write') throw new Error('no write')
    const text = written.params.text
    if (typeof text !== 'string') throw new Error('no written text')

    const start = original.indexOf(paragraph.source)
    const end = start + paragraph.source.length
    expect(start).toBeGreaterThan(0)
    expect(text.slice(0, start)).toBe(original.slice(0, start))
    expect(text.slice(start, start + 'A replacement sentence.'.length)).toBe(
      'A replacement sentence.',
    )
    expect(text.slice(start + 'A replacement sentence.'.length)).toBe(original.slice(end))
  })

  it('the conflict check compares the bytes the serialiser would write', async () => {
    const text = corpusText('crlf.md')
    const opened: NoteBuffer = { ...openBuffer({ path: 'crlf.md', text, hash: 'aaaa' }), dirty: true }

    const agree = await saveNote(
      scripted([read('crlf.md', text, 'aaaa'), write('crlf.md', 'bbbb')]).call,
      opened,
    )
    expect(agree).toEqual({ type: 'saved', hash: 'bbbb' })

    const disagree = await saveNote(
      scripted([read('crlf.md', 'another body', 'cccc')]).call,
      opened,
    )
    expect(disagree).toEqual({
      type: 'conflict',
      theirs: { text: 'another body', hash: 'cccc' },
    })
  })

  it('taking theirs and adopting an external change parse the new body', () => {
    const adopted = takeTheirs(buffer, { text: '## Theirs', hash: 'cccc' })
    expect(adopted.document).toEqual(parseDocument('## Theirs'))
    expect(adopted.dirty).toBe(false)

    const arriving = arrival({ ...buffer, dirty: false }, { text: '- their item', hash: 'dddd' })
    expect(arriving.type).toBe('adopted')
    if (arriving.type !== 'adopted') throw new Error('not adopted')
    expect(arriving.buffer.document).toEqual(parseDocument('- their item'))
  })

  it('answers a serialisation failure as an error, never a write', async () => {
    const text = '<div>raw</div>\n'
    const document = parseDocument(text)
    const block = documentBlocks(document)[0]
    if (block === undefined || block.kind !== 'opaque') throw new Error('no opaque block')
    const broken = editBlock(document, 0, block)
    const { call, seen } = scripted([])

    const outcome = await saveNote(call, {
      path: 'index.md',
      text,
      baseline: 'aaaa',
      dirty: true,
      document: broken,
    })

    expect(outcome.type).toBe('error')
    expect(seen).toEqual([])
  })
})
