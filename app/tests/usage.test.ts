import { describe, expect, it } from 'vitest'

import {
  coverage,
  dimensionLabel,
  dimensionRows,
  money,
  recordNoun,
  selectedRow,
  startedAt,
  ticketArithmetic,
  DIMENSIONS,
  tokenLine,
  tokens,
} from '../renderer/src/faces/usage'
import type { UsageReportView } from '../renderer/src/wire/UsageReportView'
import type { UsageRowView } from '../renderer/src/wire/UsageRowView'
import type { UsageSessionView } from '../renderer/src/wire/UsageSessionView'

/** The frozen fixture home's numbers, as the core's `usage.report` answers them. */
const NO_TICKET = 'no-ticket'
const TICKET_ONE = '00000000-0000-4000-8000-000000000001'
const TICKET_TWO = '00000000-0000-4000-8000-000000000002'

function session(overrides: Partial<UsageSessionView> = {}): UsageSessionView {
  return {
    id: 'ses_a',
    agent: 'builder',
    model: 'deepseek-v4.1-flash',
    startedAt: '2026-10-30T12:00:00Z',
    cost: 1,
    tokens: { input: 1000, output: 500, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
    tickets: [],
    ...overrides,
  }
}

function row(overrides: Partial<UsageRowView> = {}): UsageRowView {
  return {
    id: 'row',
    label: 'row',
    records: 1,
    reported: 1,
    cost: 1,
    barPercent: 100,
    tokens: { input: 1000, output: 500, reasoning: 0, cacheRead: 0, cacheWrite: 0 },
    sessions: [session()],
    ...overrides,
  }
}

function report(overrides: Partial<UsageReportView> = {}): UsageReportView {
  return {
    records: 4,
    reported: 3,
    cost: 2.5,
    tokens: { input: 6000, output: 1500, reasoning: 100, cacheRead: 200, cacheWrite: 50 },
    day: [
      row({
        id: '2026-10-31',
        label: '2026-10-31',
        records: 2,
        reported: 1,
        cost: 1.5,
        barPercent: 100,
        sessions: [session({ id: 'ses_b', cost: 1.5 })],
      }),
      row({
        id: '2026-10-30',
        label: '2026-10-30',
        records: 2,
        reported: 2,
        cost: 1,
        barPercent: 66.66666666666666,
        sessions: [session({ id: 'ses_a' })],
      }),
    ],
    model: [],
    agent: [
      row({ id: 'builder', label: 'builder', records: 2, reported: 2, cost: 1.75, barPercent: 100 }),
      row({
        id: 'reviewer',
        label: 'reviewer',
        cost: 0.75,
        barPercent: 42.857142857142854,
      }),
      row({ id: '', label: 'unattributed', records: 1, reported: 0, cost: 0, barPercent: 0 }),
    ],
    ticket: [
      row({
        id: TICKET_ONE,
        label: 'FIX-1: the board',
        records: 2,
        reported: 2,
        cost: 1,
        barPercent: 66.66666666666666,
      }),
      row({
        id: TICKET_TWO,
        label: 'FIX-2: the records',
        records: 2,
        reported: 1,
        cost: 0.75,
        barPercent: 50,
      }),
      row({
        id: NO_TICKET,
        label: 'no ticket',
        records: 1,
        reported: 1,
        cost: 1.5,
        barPercent: 100,
        sessions: [session({ id: 'ses_loose', cost: 1.5, tickets: [] })],
      }),
    ],
    overlap: 0.75,
    ticketTotal: 3.25,
    ...overrides,
  }
}

describe('the usage face over one report', () => {
  it('draws one dimension at a time, in the report\u2019s own order', () => {
    const drawn = report()
    expect(DIMENSIONS.map(dimensionLabel)).toEqual([
      'Day',
      'Model',
      'Agent',
      'Ticket',
    ])
    expect(dimensionRows(drawn, 'day')).toBe(drawn.day)
    expect(dimensionRows(drawn, 'agent').map((entry) => entry.label)).toEqual([
      'builder',
      'reviewer',
      'unattributed',
    ])
    expect(dimensionRows(drawn, 'ticket')).toHaveLength(3)
  })

  it('names the spend, the coverage and the ticket arithmetic from the report', () => {
    const drawn = report()
    expect(money(drawn.cost)).toBe('$2.50')
    expect(coverage(drawn)).toBe('reported by 3 of 4 records')
    expect(recordNoun(1)).toBe('1 record')
    expect(recordNoun(8)).toBe('8 records')
    expect(ticketArithmetic(drawn)).toBe(
      "Ticket rows total $3.25 \u00b7 $0.75 of that is overlap: a shared record's cost per extra ticket \u00b7 1 record carried none, $1.50",
    )
  })

  it('says a ticketless record that reported nothing without a cost', () => {
    const drawn = report({
      ticket: [
        row({ id: NO_TICKET, label: 'no ticket', records: 2, reported: 0, cost: 0, barPercent: 0 }),
      ],
      overlap: 0,
      ticketTotal: 0,
    })
    expect(ticketArithmetic(drawn)).toBe(
      'Ticket rows total $0.00 \u00b7 2 records carried none',
    )
  })

  it('states no ticket arithmetic when the report has no ticket rows', () => {
    expect(ticketArithmetic(report({ ticket: [], overlap: 0, ticketTotal: 0 }))).toBeNull()
  })

  it('draws a home whose records predate usage as zeros with a coverage of zero', () => {
    const predates = report({ records: 3, reported: 0, cost: 0, overlap: 0, ticketTotal: 0 })
    expect(money(predates.cost)).toBe('$0.00')
    expect(coverage(predates)).toBe('reported by 0 of 3 records')
  })

  it('formats tokens with grouping, and a piece no record reported as absent', () => {
    expect(tokens(6000)).toBe('6,000')
    expect(tokens(0)).toBe('0')
    expect(tokens(null)).toBe('\u2014')
  })

  it('names a record\u2019s start in the reader\u2019s zone, without seconds', () => {
    expect(startedAt('2026-10-30T12:00:00Z', 'UTC')).toBe('Oct 30, 12:00 PM')
    expect(startedAt('2026-10-31T23:30:00Z', 'UTC')).toBe('Oct 31, 11:30 PM')
  })

  it('spells a record\u2019s tokens each by its own word', () => {
    expect(tokenLine(session())).toBe('1,000 in \u00b7 500 out \u00b7 0 cache read \u00b7 0 cache write \u00b7 0 reasoning')
    expect(tokenLine(session({ tokens: null }))).toBe('no tokens reported')
    expect(tokenLine(session({ tokens: { input: null, output: 20, reasoning: null, cacheRead: null, cacheWrite: null } }))).toBe(
      '20 out',
    )
  })

  it('keeps a selected row through a reload that still holds it', () => {
    const drawn = report()
    expect(selectedRow(drawn, 'day', '2026-10-30')?.label).toBe('2026-10-30')

    const reloaded = report()
    expect(selectedRow(reloaded, 'day', '2026-10-30')?.label).toBe('2026-10-30')
    expect(selectedRow(reloaded, 'day', '2026-10-29')).toBeNull()
    expect(selectedRow(reloaded, 'agent', 'builder')?.label).toBe('builder')
    expect(selectedRow(reloaded, 'model', 'builder')).toBeNull()
  })
})
