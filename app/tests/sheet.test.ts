import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it, vi } from 'vitest'

import {
  EMPTY_COMMENT_REFUSAL,
  EMPTY_DESCRIPTION_REFUSAL,
  appendDescription,
  appendPlan,
  blocksOf,
  sheetOf,
  canSave,
  commentAuthor,
  commentParams,
  commentTime,
  commentsOf,
  detailsInitiallyOpen,
  laneChoices,
  laneCreateParams,
  laneRenameParams,
  sheetChanges,
  sheetComments,
  sheetTicket,
  shortcutHint,
  ticketHandle,
} from '../renderer/src/shell/sheet'
import { boardStatuses } from '../renderer/src/shell/board'
import type { TicketDetail } from '../renderer/src/wire/TicketDetail'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const renderer = join(appRoot, 'renderer', 'src')

function source(name: string): string {
  return readFileSync(join(renderer, name), 'utf8')
}

/**
 * A refusal's wording out of `error.rs`, with the checkout's line endings
 * normalised first: the Windows runner checks the tree out with CRLF, and a
 * `\\\n` continuation there is `\\\r\n` (PR #282's app job found this).
 */
function coreWording(error: string, arm: string): string {
  const normalized = error.replace(/\r\n/g, '\n')
  const pattern = new RegExp(
    `Self::${arm} => \\{?\\s*"([\\s\\S]*?)"\\s*\\.to_string\\(\\)`,
  )
  const match = pattern.exec(normalized)
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
    expect(EMPTY_DESCRIPTION_REFUSAL).toBe(coreWording(error, 'EmptyDescriptionBody'))
    expect(EMPTY_COMMENT_REFUSAL).toBe(coreWording(error, 'EmptyCommentBody'))
    // The Windows checkout's own shape, built from the raw bytes with `\r?\n`:
    // a line that already carries its `\r` must not be given a second one, or
    // the copy is `\r\r\n`, the normalisation leaves a `\r` behind, and the
    // continuation never matches. The copy is asserted to carry the backslash
    // and the CRLF together, so it reproduces what the runner reads.
    const crlf = error.replace(/\r?\n/g, '\r\n')
    expect(crlf).toContain('\\\r\n')
    expect(coreWording(crlf, 'EmptyDescriptionBody')).toBe(EMPTY_DESCRIPTION_REFUSAL)
    expect(coreWording(crlf, 'EmptyCommentBody')).toBe(EMPTY_COMMENT_REFUSAL)
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

  it('starts its Details closed on every opening and persists nothing', () => {
    expect(detailsInitiallyOpen()).toBe(false)
  })
})

describe('the sheet write', () => {
  it('pins an append to the revision its body was read at, not the listing\'s', () => {
    // The listing's revision can differ from the ticket read's: a concurrent
    // write between the two would be overwritten by the older body if the
    // write were pinned to the listing.
    const read = { revision: 12, ticket: ticket({ id: 't1', description: 'Stored.' }) }
    expect(appendPlan(read, 'Added.')).toEqual({
      ok: true,
      expected: 12,
      call: {
        method: 'ticket.set',
        params: {
          id: 't1',
          title: null,
          status: null,
          assignee: null,
          description: 'Stored.\n\nAdded.',
        },
      },
    })
  })

  it('keeps the revision the ticket was read at, and refuses an empty append', () => {
    const read = { revision: 42, ticket: ticket({}) }
    expect(sheetOf(read)).toEqual(read)
    expect(sheetOf({ blocks: [] })).toBeNull()
    expect(appendPlan(read, '   ')).toEqual({
      ok: false,
      message: EMPTY_DESCRIPTION_REFUSAL,
    })
  })

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
      { value: null, label: 'Uncategorized', selected: false, renamable: false },
      { value: 'Tandem', label: 'Tandem', selected: false, renamable: true },
      { value: 'Port', label: 'Port', selected: true, renamable: true },
      { value: 'Retro', label: 'Retro', selected: false, renamable: true },
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

  it('renames only a lane the store declares: a dangling name has no row to rename', () => {
    // Renaming a name only a ticket carries asks the store for a lane it does
    // not have, and `UnknownLane` is the answer.
    const dangling = laneChoices('Dangling', ['Port']).find((lane) => lane.value === 'Dangling')
    expect(dangling?.renamable).toBe(false)
    const declared = laneChoices(null, ['Port']).find((lane) => lane.value === 'Port')
    expect(declared?.renamable).toBe(true)
  })

  it('asks before it creates: a blank name makes no ask, a named one carries its key', () => {
    expect(laneCreateParams('   ', '')).toBeNull()
    expect(laneCreateParams(' Platform ', '')).toEqual({ name: 'Platform', abbr: null, project: null })
    expect(laneCreateParams('Platform', 'plat')).toEqual({ name: 'Platform', abbr: 'PLAT', project: null })
  })

  it('renames only when the name or the key moved', () => {
    expect(laneRenameParams('Port', 'Port', '')).toBeNull()
    expect(laneRenameParams('Port', '  ', '')).toBeNull()
    expect(laneRenameParams('Port', 'Port', ' prt ')).toEqual({
      from: 'Port',
      to: 'Port',
      abbr: 'PRT',
      project: null,
    })
    expect(laneRenameParams('Port', ' Platform ', '')).toEqual({
      from: 'Port',
      to: 'Platform',
      abbr: null,
      project: null,
    })
  })

  it('sheet lane picker offers the declared epic lanes', () => {
    expect(laneChoices('ui', ['Tandem', 'ui', 'core'])).toEqual([
      { value: null, label: 'Uncategorized', selected: false, renamable: false },
      { value: 'Tandem', label: 'Tandem', selected: false, renamable: true },
      { value: 'ui', label: 'ui', selected: true, renamable: true },
      { value: 'core', label: 'core', selected: false, renamable: true },
    ])
    expect(laneChoices(null, ['Tandem', 'ui', 'core'])[0]).toMatchObject({
      value: null,
      selected: true,
    })
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet).toContain('laneChoices(')
    expect(sheet).toContain('moveToLane')
    expect(sheet).toContain("method: 'ticket.move'")
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

describe('the sheet\'s shortcut hint', () => {
  it('names the platform\'s modifier, the one the key seam reads', () => {
    vi.stubGlobal('navigator', { platform: 'Win32' })
    expect(shortcutHint()).toBe('Ctrl+↩ saves · Ctrl+⇧↩ posts · Esc closes')

    vi.stubGlobal('navigator', { platform: 'Linux x86_64' })
    expect(shortcutHint()).toBe('Ctrl+↩ saves · Ctrl+⇧↩ posts · Esc closes')

    vi.stubGlobal('navigator', { platform: 'MacIntel' })
    expect(shortcutHint()).toBe('⌘↩ saves · ⌘⇧↩ posts · Esc closes')
    vi.unstubAllGlobals()
  })

  it('draws the hint through the seam, and prints no bare ⌘ anywhere else', () => {
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet).toContain('{shortcutHint()}')

    const stray = readdirSync(join(renderer), { recursive: true, encoding: 'utf8' })
      // `readdirSync` joins with the checkout's separator: the Windows runner
      // hands back `shell\keys.ts`.
      .map((name) => name.split(/[\\/]/).join('/'))
      .filter((name) => /\.(ts|tsx)$/.test(name))
      .filter((name) => name !== 'shell/keys.ts')
      .filter((name) => readFileSync(join(renderer, name), 'utf8').includes('⌘'))
    expect(stray).toEqual([])
  })
})

describe('the sheet copy (TAND-13)', () => {
  it('names the sections Description, Comments and Add a comment, without the restated copy', () => {
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet).toContain('>Description</h2>')
    expect(sheet).not.toContain('Description: markdown, saved with the ticket')
    expect(sheet).toContain('>Comments</h2>')
    expect(sheet).not.toContain("Comments: the ticket's discussion log")
    expect(sheet).toContain('Add a comment')
    expect(sheet).not.toContain('Add a comment: appended to the log, signed as you')
  })

  it('reads append-only in one glance: no Edit source, a View source plus an append-only note', () => {
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet).not.toContain('Edit source')
    expect(sheet).toContain('View source')
    expect(sheet).toContain('Append-only')
    expect(sheet).toContain('sheet__note')
    expect(sheet).toContain('Append')
  })

  it('keeps one Save path for the fields', () => {
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet.match(/sheet__save/g)?.length).toBe(1)
  })
})

describe('the sheet sticky title (TAND-13)', () => {
  it('holds the title and Save in a sticky bar at the top of the scroll', () => {
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet).toContain('sheet__sticky')
    const stickyAt = sheet.indexOf('sheet__sticky')
    const titleAt = sheet.indexOf('sheet__title')
    const saveAt = sheet.indexOf('sheet__save')
    const bodyAt = sheet.indexOf('sheet__body')
    expect(stickyAt).toBeGreaterThanOrEqual(0)
    expect(titleAt).toBeGreaterThan(stickyAt)
    expect(saveAt).toBeGreaterThan(stickyAt)
    // The sticky bar sits inside the scrolling body, so it can stick.
    expect(bodyAt).toBeLessThan(stickyAt)

    const css = readFileSync(join(renderer, 'app.css'), 'utf8')
    expect(css).toContain('.sheet__sticky')
    expect(css).toMatch(/\.sheet__sticky[^}]*position:\s*sticky/)
  })
})

describe('the sheet status colour (TAND-13)', () => {
  it('carries the list state colour on the status control', () => {
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet).toContain('sheet__status-dot')
    expect(sheet).toContain('data-status={status}')

    const css = readFileSync(join(renderer, 'app.css'), 'utf8')
    for (const [status, token] of [
      ['on-hold', '--ember-deep'],
      ['ready', '--accent-deep'],
      ['in-progress', '--accent'],
      ['in-review', '--moss'],
      ['done', '--ink-dim'],
    ] as const) {
      expect(css).toContain(`.sheet__status-dot[data-status="${status}"]`)
      const listRule = new RegExp(`\\.ticket-row__status\\[data-status="${status}"\\][^}]*${token}`)
      expect(css, `list maps ${status} to ${token}`).toMatch(listRule)
      const sheetRule = new RegExp(
        `\\.sheet__status-dot\\[data-status="${status}"\\][^}]*${token}`,
      )
      expect(css, `sheet reuses ${status} as ${token}`).toMatch(sheetRule)
    }
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
        ['faces/TicketSheet.tsx']
          .flatMap((name) => [...source(name).matchAll(/method: '([^']+)'/g)])
          .flatMap((match) => (match[1] === undefined ? [] : [match[1]])),
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

describe('the sheet reads under payload drift (TAN-312)', () => {
  it('reads no ticket out of a reply that is not an object', () => {
    expect(sheetOf(null as never)).toBeNull()
    expect(sheetOf(undefined as never)).toBeNull()
    expect(sheetOf('ticket' as never)).toBeNull()
    expect(sheetOf({} as never)).toBeNull()
    expect(sheetOf({ ticket: null, revision: 1 } as never)).toBeNull()
    expect(sheetOf({ ticket: { id: 't' }, revision: null } as never)).toBeNull()
  })

  it('reads no comments or blocks out of a reply that is not an object', () => {
    expect(commentsOf(null as never)).toBeNull()
    expect(commentsOf(undefined as never)).toBeNull()
    expect(blocksOf(null as never)).toBeNull()
    expect(blocksOf(undefined as never)).toBeNull()
    expect(commentsOf({} as never)).toBeNull()
    expect(blocksOf({} as never)).toBeNull()
  })

  it('reads no comments or blocks out of a reply whose value is not an array', () => {
    expect(commentsOf({ comments: null } as never)).toBeNull()
    expect(blocksOf({ blocks: 'x' } as never)).toBeNull()
  })

  it('reads the comments and blocks the store holds, as before', () => {
    expect(commentsOf({ comments: [{ at: 'a', author: 'b', body: 'c' }] } as never)).toEqual([
      { at: 'a', author: 'b', body: 'c' },
    ])
    expect(blocksOf({ blocks: [] } as never)).toEqual([])
  })

  it('names an unnamed comment author, even when the name is missing', () => {
    expect(commentAuthor(null as never)).toBe('anonymous')
    expect(commentAuthor(undefined as never)).toBe('anonymous')
  })

  it('writes no stamp for a missing comment time, instead of throwing', () => {
    expect(commentTime(null as never)).toBe('')
    expect(commentTime(undefined as never)).toBe('')
  })

  it('handles a ticket with no ref and no number, even when both are missing', () => {
    expect(ticketHandle({ ref: undefined, number: undefined } as never)).toBe('#?')
  })

  it('lists the default area when the lane listing is missing, instead of throwing', () => {
    expect(laneChoices(null, undefined as never)).toEqual([
      { value: null, label: 'Uncategorized', selected: true, renamable: false },
    ])
  })

  it('draws a ticket whose edges and description drifted, instead of throwing', () => {
    const drifting = ticket({
      blockedBy: null as never,
      blocks: [{ id: 'edge', ref: 'E-1', title: 'Edge', status: 'ready' }, null] as never,
      description: 42 as never,
    })
    const drawn = sheetTicket(drifting)
    expect(drawn.blockedBy).toEqual([])
    expect(drawn.blocks).toEqual([{ id: 'edge', ref: 'E-1', title: 'Edge', status: 'ready' }])
    expect(drawn.description).toBeNull()
    expect(drawn.title).toBe(drifting.title)
  })

  it('draws log rows whose fields drifted, instead of throwing', () => {
    expect(
      sheetComments([
        { at: null, author: null, body: null },
        null,
        { at: '2026-11-04T15:39:55Z', author: 'sprint', body: 'Steady.' },
      ] as never),
    ).toEqual([
      { at: '', author: '', body: '' },
      { at: '2026-11-04T15:39:55Z', author: 'sprint', body: 'Steady.' },
    ])
  })
})

describe('the sheet metadata row (UI-34)', () => {
  it('draws the row above the description, inside the sticky band', () => {
    const sheet = source('faces/TicketSheet.tsx')
    const bodyAt = sheet.indexOf('sheet__body')
    const stickyAt = sheet.indexOf('sheet__sticky')
    const metaAt = sheet.indexOf('sheet__meta')
    const descriptionAt = sheet.indexOf('sheet__description')
    expect(stickyAt).toBeGreaterThan(bodyAt)
    expect(metaAt).toBeGreaterThan(stickyAt)
    expect(descriptionAt).toBeGreaterThan(metaAt)
  })

  it('the_sheet_status_menu_carries_the_six_fixed_statuses', () => {
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet).toContain('boardStatuses().map((entry) => (')
    expect(boardStatuses().map((status) => status.value)).toEqual([
      'backlog',
      'on-hold',
      'ready',
      'in-progress',
      'in-review',
      'done',
    ])
  })

  it('reuses the sheet helpers, with no second option list beside them', () => {
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet.match(/boardStatuses\(/g)?.length).toBe(1)
    expect(sheet.match(/laneChoices\(/g)?.length).toBe(1)
    expect(sheet).toContain('moveToLane')
    expect(sheet).toContain("method: 'ticket.move'")
  })

  it('names each control\'s write in sight, so two save semantics are legible', () => {
    const sheet = source('faces/TicketSheet.tsx')
    expect(sheet).toContain('saves with Save')
    expect(sheet).toContain('moves at once')
    // The lane picker writes through the move path, never a fabricated set.
    const metaAt = sheet.indexOf('sheet__meta')
    const laneAt = sheet.indexOf('aria-label="Lane"')
    expect(laneAt).toBeGreaterThan(metaAt)
  })
})
