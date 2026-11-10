import { describe, expect, it } from 'vitest'

import { parseRoute } from '../renderer/src/shell/route'
import {
  LONG_OUTPUT_LINES,
  isLongOutput,
  lineCount,
  outputPreview,
  resolveInitialExpanded,
  sessionHref,
  sessionsOf,
  sheetSessionHref,
  unavailableNotice,
  usageLine,
} from '../renderer/src/shell/sessions'
import type { Session } from '../renderer/src/wire/Session'

function session(overrides: Partial<Session>): Session {
  return {
    id: 'ses_new',
    harnessSessionId: 'ses_harness',
    harnessTransport: 'database',
    project: 'tandem',
    agent: 'builder',
    brief: 'A brief.',
    summary: 'What landed.',
    status: 'ended',
    outcome: 'succeeded',
    tickets: [],
    startedAt: '2026-10-30T12:00:00Z',
    endedAt: '2026-10-30T12:05:00Z',
    tokens: { input: 25765, output: 3100, reasoning: null, cacheRead: null, cacheWrite: null },
    cost: 0.42,
    model: 'deepseek-v4',
    skills: null,
    turns: null,
    calls: null,
    ...overrides,
  }
}

describe('the session deep link', () => {
  it('opens the sheet with the linked session expanded', () => {
    expect(parseRoute('#/tickets?ticket=T1&session=S1')).toEqual({
      tab: 'tickets',
      view: 'list',
      ticket: 'T1',
      session: 'S1',
    })
  })

  it('reads no session when the query names none, or names it blank', () => {
    expect(parseRoute('#/tickets?ticket=T1').session).toBeNull()
    expect(parseRoute('#/tickets?ticket=T1&session=').session).toBeNull()
    expect(parseRoute('#/tickets?ticket=T1&other=S1').session).toBeNull()
  })

  it('answers no session for a query it cannot decode, instead of crashing', () => {
    expect(parseRoute('#/tickets?ticket=T1&session=%zz').session).toBeNull()
    expect(() => parseRoute('#/tickets?ticket=T1&session=%zz')).not.toThrow()
  })

  it('keeps the session beside the ticket in one kept link', () => {
    expect(sessionHref('T1', 'S1')).toBe('#/tickets?ticket=T1&session=S1')
    expect(parseRoute(sessionHref('T 1', 'S/1'))).toEqual({
      tab: 'tickets',
      view: 'list',
      ticket: 'T 1',
      session: 'S/1',
    })
  })

  it('swaps the expanded row without moving the tab or the layout', () => {
    expect(sheetSessionHref('#/bikeshed?ticket=T', 'T', 'S')).toBe('#/bikeshed?ticket=T&session=S')
    expect(sheetSessionHref('#/tickets/board?ticket=T&session=S', 'T', null)).toBe(
      '#/tickets/board?ticket=T',
    )
    expect(sheetSessionHref('#/tickets?ticket=T&session=A', 'T', 'B')).toBe(
      '#/tickets?ticket=T&session=B',
    )
  })
})

describe('the sessions section', () => {
  it('expands the deep-linked session when it is linked, and ignores a stray sid with a notice', () => {
    expect(resolveInitialExpanded(['a', 'b'], 'b')).toEqual({ expanded: 'b', notice: null })
    expect(resolveInitialExpanded(['a', 'b'], null)).toEqual({ expanded: null, notice: null })
    expect(resolveInitialExpanded(['a'], 'stray')).toEqual({
      expanded: null,
      notice: 'No session stray is linked to this ticket.',
    })
    expect(resolveInitialExpanded([], 'stray')).toEqual({
      expanded: null,
      notice: 'No session stray is linked to this ticket.',
    })
  })

  it('says why a transcript is missing, on the row, without breaking the section', () => {
    expect(unavailableNotice('the harness database is missing')).toBe(
      'transcript unavailable: the harness database is missing',
    )
  })

  it('collapses a tool output past fifty lines behind an expand control', () => {
    const lines = Array.from({ length: 61 }, (_, index) => `line ${index + 1}`)
    const text = lines.join('\n')
    expect(LONG_OUTPUT_LINES).toBe(50)
    expect(lineCount(text)).toBe(61)
    expect(isLongOutput(text)).toBe(true)
    expect(isLongOutput(lines.slice(0, 50).join('\n'))).toBe(false)
    const preview = outputPreview(text)
    expect(lineCount(preview)).toBe(50)
    expect(preview.endsWith('line 50')).toBe(true)
  })

  it('draws the record header the store holds: agent, outcome, model, cost, tokens', () => {
    expect(usageLine(session({}))).toBe('deepseek-v4 · $0.42 · 25765 in / 3100 out')
    expect(usageLine(session({ model: null, cost: null, tokens: null }))).toBeNull()
    expect(usageLine(session({ model: 'deepseek-v4', cost: null, tokens: null }))).toBe(
      'deepseek-v4',
    )
  })

  it('reads no sessions out of a reply that is not an object (TAN-312)', () => {
    expect(sessionsOf(null as never)).toBeNull()
    expect(sessionsOf(undefined as never)).toBeNull()
    expect(sessionsOf({} as never)).toBeNull()
    expect(sessionsOf({ sessions: null } as never)).toBeNull()
    expect(sessionsOf({ sessions: [{ id: 's1' }] } as never)).toEqual([{ id: 's1' }])
  })

  it('draws the record header when the usage fields are missing (TAN-312)', () => {
    expect(
      usageLine(session({ model: undefined, cost: 'free', tokens: undefined } as never)),
    ).toBeNull()
    expect(usageLine(session({ model: '', cost: NaN, tokens: null }))).toBeNull()
    expect(usageLine(session({ cost: 1.5, model: null, tokens: null }))).toBe('$1.50')
  })
})
