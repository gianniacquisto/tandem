import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import {
  EMPTY_COMMENT_REFUSAL,
  EMPTY_DESCRIPTION_REFUSAL,
  answerBlock,
  answered,
  appendDescription,
  assigneeChoices,
  canSave,
  commentAuthor,
  commentParams,
  commentTime,
  holdOpen,
  laneChoices,
  laneCreateParams,
  laneRenameParams,
  sheetChanges,
  ticketHandle,
} from '../renderer/src/shell/sheet'
import type { TicketDetail } from '../renderer/src/wire/TicketDetail'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const renderer = join(appRoot, 'renderer', 'src')

function source(name: string): string {
  return readFileSync(join(renderer, name), 'utf8')
}

/**
 * The refusal wording out of `error.rs`, with the checkout's line endings
 * normalised first: the Windows runner checks the tree out with CRLF, and a
 * `\\\n` continuation there is `\\\r\n` (PR #282's app job found this).
 */
function wordingFrom(error: string): string {
  const normalized = error.replace(/\r\n/g, '\n')
  const match = /Self::EmptyDescriptionBody => "([\s\S]*?)"\s*\.to_string\(\)/.exec(normalized)
  return (match?.[1] ?? '')
    .replace(/\\\n\s+/g, ' ')
    .replace(/\\"/g, '"')
    .replace(/\s+/g, ' ')
    .trim()
}

function ticket(overrides: Partial<TicketDetail>): TicketDetail {
  return {
    archived: false,
    assignee: null,
    blocked: false,
    blockedBy: [],
    blockedReason: null,
    blocks: [],
    completedAt: null,
    createdAt: '2026-10-30T08:00:00Z',
    description: null,
    done: false,
    id: 'D5BAD6F7-5D45-4910-9A69-1C0409479BAD',
    lane: 'Port',
    link: null,
    number: 17,
    origin: 'refiner',
    ref: 'PRT-17',
    status: 'ready',
    title: 'Port: the ticket sheet and the lane surface',
    ...overrides,
  }
}

describe('the sheet description', () => {
  it('appends an addition under the stored body, separated by one blank line', () => {
    expect(appendDescription('## Why\n\nThe sheet.', 'A new fact.')).toEqual({
      ok: true,
      body: '## Why\n\nThe sheet.\n\nA new fact.',
    })
    expect(appendDescription(null, '  The first body.  ')).toEqual({
      ok: true,
      body: 'The first body.',
    })
    expect(appendDescription('   ', 'Text.')).toEqual({ ok: true, body: 'Text.' })
  })

  it('refuses an empty addition in the domain\'s own words, and writes nothing', () => {
    expect(appendDescription('A body.', '   ')).toEqual({
      ok: false,
      message: EMPTY_DESCRIPTION_REFUSAL,
    })
    expect(appendDescription('A body.', '')).toEqual({
      ok: false,
      message: EMPTY_DESCRIPTION_REFUSAL,
    })
  })

  it('carries the CLI\'s refusal text, not a second wording of it', () => {
    // The one wording is the core's (`EmptyDescriptionBody`): read it out of
    // the Rust source and fail when the sheet's copy drifts from it. The parse
    // normalises line endings first, so the Windows checkout reads the same
    // arm as the macOS one.
    const error = readFileSync(
      join(appRoot, '..', 'rust', 'core', 'src', 'domain', 'error.rs'),
      'utf8',
    )
    expect(EMPTY_DESCRIPTION_REFUSAL).toBe(wordingFrom(error))
    // The Windows checkout's own shape, built from the raw bytes with `\r?\n`:
    // a line that already carries its `\r` must not be given a second one, or
    // the copy is `\r\r\n`, the normalisation leaves a `\r` behind, and the
    // continuation never matches. The copy is asserted to carry the backslash
    // and the CRLF together, so it reproduces what the runner reads.
    const crlf = error.replace(/\r?\n/g, '\r\n')
    expect(crlf).toContain('\\\r\n')
    expect(wordingFrom(crlf)).toBe(EMPTY_DESCRIPTION_REFUSAL)
  })

  it('joins the stored body and the addition with the core\'s one blank line', () => {
    expect(appendDescription('One.', 'Two.')).toEqual({ ok: true, body: 'One.\n\nTwo.' })
    // The sheet restates `board.rs`\'s `appending`; this pin keeps the two
    // copies of the separator from drifting.
    const board = readFileSync(
      join(appRoot, '..', 'rust', 'core', 'src', 'domain', 'board.rs'),
      'utf8',
    )
    expect(board).toContain('format!("{existing}\\n\\n{section}")')
  })

  it('counts the heading alone as an answer, the way the store does', () => {
    expect(answered('## Answer')).toBe(true)
    expect(answered('## Answer\n\n')).toBe(true)
    expect(answered('## Why\n\nThe question.')).toBe(false)
    expect(answered('Mentions ## Answer inline, but is not a block.')).toBe(false)
    expect(answered(null)).toBe(false)
  })

  it('finds an answer block by its line-anchored heading, and the body under it', () => {
    const answered = '## Why\n\nBecause.\n\n## Answer — 2026-10-30\n\nDo the thing.\n\nMore.\n\n## Done when\n\n- It works.'
    expect(answerBlock(answered)).toBe('Do the thing.\n\nMore.')
    expect(answerBlock('## Why\n\nBecause.')).toBeNull()
    expect(answerBlock('Mentions ## Answer inline, but is not a block.')).toBeNull()
    expect(answerBlock(null)).toBeNull()
    expect(answerBlock('## answer\n\nLower case still counts.')).toBe('Lower case still counts.')
  })

  it('calls a hold open until its answer block is written', () => {
    expect(holdOpen(ticket({ status: 'on-hold' }))).toBe(true)
    expect(holdOpen(ticket({ status: 'on-hold', description: '## Answer\n\nYes.' }))).toBe(false)
    // The core counts the heading alone as answered (`DescriptionBody::answered`),
    // so the sheet must not draw a hold over a ticket the store calls answered.
    expect(holdOpen(ticket({ status: 'on-hold', description: '## Answer' }))).toBe(false)
    expect(holdOpen(ticket({ status: 'ready' }))).toBe(false)
  })
})

describe('the sheet write', () => {
  it('sends only the fields that moved, and nothing when nothing did', () => {
    const stored = ticket({ title: 'A title', status: 'ready', assignee: 'builder' })

    expect(sheetChanges(stored, { title: 'A title', status: 'ready', assignee: 'builder' })).toEqual(
      null,
    )
    expect(
      sheetChanges(stored, { title: 'A better title', status: 'ready', assignee: 'builder' }),
    ).toEqual({ title: 'A better title' })
    expect(
      sheetChanges(stored, { title: 'A title', status: 'in-progress', assignee: 'human' }),
    ).toEqual({ status: 'in-progress', assignee: 'human' })
  })

  it('trims the title on its way out, and sends the clear as the empty string', () => {
    const stored = ticket({ title: 'A title', assignee: 'builder' })

    expect(sheetChanges(stored, { title: '  New  ', status: 'ready', assignee: null })).toEqual({
      title: 'New',
      assignee: '',
    })
  })

  it('refuses to save a sheet with no title, as the domain would', () => {
    expect(canSave({ title: 'A title' })).toBe(true)
    expect(canSave({ title: '   ' })).toBe(false)
  })
})

describe('the sheet fields', () => {
  it('offers the assignee picker in the Swift order: unassigned, human, roster, off-roster current', () => {
    expect(assigneeChoices(['builder', 'human', ' review '], 'review')).toEqual([
      { value: null, label: 'Unassigned', selected: false },
      { value: 'human', label: 'human', selected: false },
      { value: 'builder', label: '@builder', selected: false },
      { value: 'review', label: '@review', selected: true },
    ])
    expect(assigneeChoices([], null)[0]?.selected).toBe(true)
  })

  it('takes the ticket handle the human writes, or the number when it has no ref', () => {
    expect(ticketHandle(ticket({}))).toBe('PRT-17')
    expect(ticketHandle(ticket({ ref: null, number: null }))).toBe('#?')
  })

  it('names the refusals the domain owns', () => {
    expect(EMPTY_COMMENT_REFUSAL).toBe(
      'Comment body is empty. Provide a non-empty body and try again.',
    )
  })
})

describe('the lane surface', () => {
  it('lists the default area and every cycle lane, marking the one the ticket is in', () => {
    expect(laneChoices('Port', ['Tandem', 'Port', 'Retro'])).toEqual([
      { value: null, label: 'Uncategorized', selected: false },
      { value: 'Tandem', label: 'Tandem', selected: false },
      { value: 'Port', label: 'Port', selected: true },
      { value: 'Retro', label: 'Retro', selected: false },
    ])
    expect(laneChoices(null, ['Port'])[0]?.selected).toBe(true)
  })

  it('keeps a lane no declaration names, so a hand edit cannot hide it', () => {
    expect(laneChoices('Dangling', ['Port']).map((lane) => lane.value)).toEqual([
      null,
      'Port',
      'Dangling',
    ])
  })

  it('asks before it creates: a blank name makes no ask, a named one carries its key', () => {
    expect(laneCreateParams('   ', '')).toBeNull()
    expect(laneCreateParams(' Platform ', '')).toEqual({ name: 'Platform', abbr: null })
    expect(laneCreateParams('Platform', 'plat')).toEqual({ name: 'Platform', abbr: 'PLAT' })
  })

  it('renames only when the name or the key moved', () => {
    expect(laneRenameParams('Port', 'Port', '')).toBeNull()
    expect(laneRenameParams('Port', '  ', '')).toBeNull()
    expect(laneRenameParams('Port', 'Port', ' prt ')).toEqual({
      from: 'Port',
      to: 'Port',
      abbr: 'PRT',
    })
    expect(laneRenameParams('Port', ' Platform ', '')).toEqual({
      from: 'Port',
      to: 'Platform',
      abbr: null,
    })
  })
})

describe('the sheet comments', () => {
  it('posts the human\'s comment as the human, trimmed', () => {
    expect(commentParams('id', '  A note.  ')).toEqual({
      id: 'id',
      author: 'human',
      body: 'A note.',
    })
    expect(commentParams('id', '   ')).toBeNull()
  })

  it('names an unnamed author rather than leaving a gap', () => {
    expect(commentAuthor('')).toBe('anonymous')
    expect(commentAuthor('usage')).toBe('usage')
  })

  it('writes a comment stamp as the stored date and minute', () => {
    expect(commentTime('2026-10-31T14:17:40Z')).toBe('2026-10-31 14:17')
  })
})

describe('the sheet\'s methods', () => {
  it('calls only methods the wire declares, so a UI-only write cannot appear', () => {
    const method = readFileSync(join(renderer, 'wire', 'Method.ts'), 'utf8')
    const known = new Set(
      [...method.matchAll(/"method": "([^"]+)"/g)].flatMap((match) =>
        match[1] === undefined ? [] : [match[1]],
      ),
    )

    const calls = [
      ...new Set(
        [...source('faces/TicketSheet.tsx').matchAll(/method: '([^']+)'/g)].flatMap((match) =>
          match[1] === undefined ? [] : [match[1]],
        ),
      ),
    ].sort()

    expect(calls).toEqual([
      'lane.add',
      'lane.rename',
      'markdown.blocks',
      'ticket.comment.add',
      'ticket.comment.list',
      'ticket.move',
      'ticket.set',
      'ticket.show',
    ])
    for (const tag of calls) expect(known, `TicketSheet calls ${tag}`).toContain(tag)
  })
})
