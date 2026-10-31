import { describe, expect, it } from 'vitest'

import { initialSnapshot, snapshotReducer, type SnapshotState } from '../renderer/src/shell/snapshot'
import { refusalBanner, writeBlocked } from '../renderer/src/shell/write'
import type { TicketSummary } from '../renderer/src/wire/TicketSummary'
import type { UsageReportView } from '../renderer/src/wire/UsageReportView'

const ticket: TicketSummary = {
  archived: false,
  assignee: null,
  blocked: false,
  blockedBy: [],
  blocks: [],
  completedAt: null,
  createdAt: '2026-10-30T08:00:00Z',
  descriptionLength: 0,
  done: false,
  id: 'one',
  lane: 'Port',
  link: null,
  number: 1,
  origin: 'human',
  ref: 'PRT-1',
  status: 'ready',
  title: 'a ticket',
}

const report: UsageReportView = {
  records: 4,
  reported: 3,
  cost: 2.5,
  tokens: { input: null, output: null, reasoning: null, cacheRead: null, cacheWrite: null },
  day: [],
  model: [],
  agent: [],
  ticket: [],
  overlap: 0.75,
  ticketTotal: 3.25,
}

function ready(): SnapshotState {
  return snapshotReducer(initialSnapshot, {
    type: 'loaded',
    reply: { revision: 12, tickets: [ticket] },
    lanes: ['Port'],
  })
}

describe('the shell snapshot', () => {
  it('starts connecting, with no revision of its own and nothing drawn', () => {
    expect(initialSnapshot).toEqual({
      status: 'connecting',
      revision: null,
      tickets: [],
      lanes: [],
      usage: null,
      error: null,
      notice: null,
      stale: false,
    })
  })

  it('stores the listing: its revision, its tickets and the lanes', () => {
    const next = ready()

    expect(next.status).toBe('ready')
    expect(next.revision).toBe(12)
    expect(next.tickets).toEqual([ticket])
    expect(next.lanes).toEqual(['Port'])
    expect(next.error).toBeNull()
    expect(next.stale).toBe(false)
  })

  it('keeps the last revision and the rows when a read fails, so the face does not lie', () => {
    const failed = snapshotReducer(ready(), { type: 'failed', message: 'the store is locked' })

    expect(failed.status).toBe('failed')
    expect(failed.revision).toBe(12)
    expect(failed.tickets).toEqual([ticket])
    expect(failed.error).toBe('the store is locked')
  })

  it('marks the snapshot stale when an event names a store the listing has not caught up to', () => {
    const stale = snapshotReducer(ready(), { type: 'stale' })

    expect(stale.stale).toBe(true)
    expect(stale.revision).toBe(12)

    const caughtUp = snapshotReducer(stale, {
      type: 'loaded',
      reply: { revision: 13, tickets: [ticket] },
      lanes: ['Port'],
    })
    expect(caughtUp.stale).toBe(false)
    expect(caughtUp.revision).toBe(13)
  })

  it('carries the one usage report the Usage face draws', () => {
    const next = snapshotReducer(initialSnapshot, { type: 'usage', report })

    expect(next.usage).toBe(report)
    expect(next.revision).toBeNull()
  })

  it('keeps the last report when the next read fails', () => {
    const loaded = snapshotReducer(initialSnapshot, { type: 'usage', report })
    const failed = snapshotReducer(loaded, { type: 'failed', message: 'no store' })

    expect(failed.usage).toBe(report)
  })

  it('shows a conflict until it is dismissed, and a load does not wipe it', () => {
    const noticed = snapshotReducer(ready(), { type: 'notice', message: 'the board moved' })
    expect(noticed.notice).toBe('the board moved')

    const reloaded = snapshotReducer(noticed, {
      type: 'loaded',
      reply: { revision: 13, tickets: [ticket] },
      lanes: ['Port'],
    })
    expect(reloaded.notice).toBe('the board moved')

    expect(snapshotReducer(reloaded, { type: 'dismiss' }).notice).toBeNull()
  })
})

describe('the write guard', () => {
  it('refuses before the first read lands', () => {
    expect(writeBlocked(initialSnapshot, null)).toMatch(/not been read/)
  })

  it('refuses while the last read failed', () => {
    const failed = snapshotReducer(ready(), { type: 'failed', message: 'the store is locked' })
    expect(writeBlocked(failed, 12)).toMatch(/not readable/)
  })

  it('refuses a write against a snapshot an event has invalidated', () => {
    const stale = snapshotReducer(ready(), { type: 'stale' })
    expect(writeBlocked(stale, 12)).toMatch(/changed while you were acting/)
  })

  it('refuses a write aimed at a revision the listing no longer shows', () => {
    expect(writeBlocked(ready(), 11)).toMatch(/revision 11 .* revision 12/)
  })

  it('lets a write through on the revision the face rendered', () => {
    expect(writeBlocked(ready(), 12)).toBeNull()
    expect(writeBlocked(ready(), null)).toBeNull()
  })

  it("answers a sheet's refusal in the sheet, never on the shell's banner", () => {
    // The sheet draws the same words beside the field that raised them; the
    // shell's banner behind the modal is a second copy that survives Esc.
    expect(refusalBanner('sheet', 'The board changed.')).toBeNull()
    expect(refusalBanner('face', 'The board changed.')).toBe('The board changed.')
  })
})
