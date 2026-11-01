import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import {
  countsByPass,
  edgeHandoffs,
  edgePath,
  nodeForStatus,
  nodeFrame,
  PASSES,
  positionBatch,
  type PassId,
} from '../renderer/src/faces/workflow'
import type { Sprint } from '../renderer/src/wire/Sprint'
import type { TicketSummary } from '../renderer/src/wire/TicketSummary'

const appRoot = fileURLToPath(new URL('..', import.meta.url))
const renderer = join(appRoot, 'renderer', 'src')

function source(name: string): string {
  return readFileSync(join(renderer, name), 'utf8')
}

/** The method tags `Method.ts` declares: the wire's whole surface. */
function wireMethods(): Set<string> {
  const method = readFileSync(join(renderer, 'wire', 'Method.ts'), 'utf8')
  const tags = new Set<string>()
  for (const match of method.matchAll(/"method": "([^"]+)"/g)) {
    if (match[1] !== undefined) tags.add(match[1])
  }
  return tags
}

/** The method tags a source names at a call site. */
function calledMethods(text: string): string[] {
  return [...text.matchAll(/method: '([^']+)'/g)].flatMap((match) =>
    match[1] === undefined ? [] : [match[1]],
  )
}

function ticket(id: string, status: string, overrides: Partial<TicketSummary> = {}): TicketSummary {
  return {
    archived: false,
    assignee: null,
    blocked: false,
    blockedBy: [],
    blocks: [],
    completedAt: null,
    createdAt: '2026-10-29T12:00:00Z',
    descriptionLength: 10,
    done: status === 'done',
    id,
    lane: 'Tandem',
    link: null,
    number: 1,
    origin: 'human',
    ref: null,
    status,
    title: `Ticket ${id}`,
    ...overrides,
  }
}

/** The worked open record the positioning reads: six batch tickets, one per status. */
const TICKETS: TicketSummary[] = [
  ticket('t-backlog', 'backlog'),
  ticket('t-ready', 'ready'),
  ticket('t-progress', 'in-progress'),
  ticket('t-review', 'in-review'),
  ticket('t-hold', 'on-hold'),
  ticket('t-done', 'done'),
]

const SPRINT: Sprint = {
  lane: 'Tandem',
  number: 2,
  session: 'ses_b',
  mode: 'parallel',
  batch: ['t-backlog', 't-ready', 't-progress', 't-review', 't-hold', 't-done', 't-gone'],
  runs: ['2026-10-30T12:00:00Z, build landed t-done'],
  close: null,
  status: 'open',
  openedAt: '2026-10-30T09:00:00Z',
  closedAt: null,
}

describe('the workflow DAG', () => {
  it('walks the sprint loop in order: refine, build, review, retro, done', () => {
    expect(PASSES.map((pass) => pass.id)).toEqual([
      'refine',
      'build',
      'review',
      'retro',
      'done',
    ])
    expect(edgeHandoffs()).toEqual([
      ['refine', 'build'],
      ['build', 'review'],
      ['review', 'retro'],
      ['retro', 'done'],
    ])
  })

  it('positions batch tickets by status: shaping at refine, claims at build, gates at review', () => {
    expect(nodeForStatus('backlog')).toBe('refine')
    expect(nodeForStatus('ready')).toBe('refine')
    expect(nodeForStatus('in-progress')).toBe('build')
    expect(nodeForStatus('in-review')).toBe('review')
  })

  it('holds parked work at retro and landed work at done', () => {
    // The retro reads the whole batch at the close, so what slipped and what
    // waits (on-hold) sits at its node; done work rests at the terminal.
    expect(nodeForStatus('on-hold')).toBe('retro')
    expect(nodeForStatus('done')).toBe('done')
  })

  it('parks an unknown status at the front of the pipe, never off the graph', () => {
    expect(nodeForStatus('polish')).toBe('refine')
    expect(nodeForStatus('')).toBe('refine')
  })

  it('keeps the batch order and names a batch id the board no longer holds', () => {
    const placed = positionBatch(SPRINT.batch, TICKETS)

    expect(placed.map((one) => one.id)).toEqual(SPRINT.batch)
    expect(placed.map((one) => one.pass)).toEqual([
      'refine',
      'refine',
      'build',
      'review',
      'retro',
      'done',
      'refine',
    ])
    // `t-gone` left the board after the draw: no summary, still positioned, so
    // the face can name it instead of dropping it silently.
    const gone = placed.find((one) => one.id === 't-gone')
    expect(gone?.ticket).toBeNull()
    expect(gone?.pass).toBe('refine')
    expect(placed.find((one) => one.id === 't-done')?.ticket?.status).toBe('done')
  })

  it('counts the batch per node for the drawn tallies', () => {
    const counts: Record<PassId, number> = countsByPass(positionBatch(SPRINT.batch, TICKETS))

    expect(counts).toEqual({ refine: 3, build: 1, review: 1, retro: 1, done: 1 })
  })

  it('lays the nodes left to right on one row, with room for the batch chips', () => {
    const frames = PASSES.map((_, index) => nodeFrame(index))

    for (const frame of frames) {
      expect(frame.y).toBe(frames[0]?.y)
      expect(frame.width).toBeGreaterThan(0)
      expect(frame.height).toBeGreaterThan(0)
    }
    for (let index = 1; index < frames.length; index++) {
      expect(frames[index]?.x ?? 0).toBeGreaterThan(frames[index - 1]?.x ?? 0)
    }
    // No overlap: each node starts past the previous one's right edge.
    for (let index = 1; index < frames.length; index++) {
      const before = frames[index - 1]
      const frame = frames[index]
      expect(frame?.x ?? 0).toBeGreaterThanOrEqual((before?.x ?? 0) + (before?.width ?? 0))
    }
  })

  it('draws each handoff left to right, into the next node', () => {
    for (const [from, to] of edgeHandoffs()) {
      const fromIndex = PASSES.findIndex((pass) => pass.id === from)
      const toIndex = PASSES.findIndex((pass) => pass.id === to)
      const fromFrame = nodeFrame(fromIndex)
      const toFrame = nodeFrame(toIndex)
      const path = edgePath(fromFrame, toFrame)

      expect(toIndex).toBe(fromIndex + 1)
      expect(path).toMatch(/^M [-0-9.]+ [-0-9.]+ L [-0-9.]+ [-0-9.]+$/)
      const parts = path.split(' ')
      expect(Number(parts[1] ?? NaN)).toBe(fromFrame.x + fromFrame.width)
      expect(Number(parts[4] ?? NaN)).toBe(toFrame.x)
      expect(Number(parts[4] ?? 0)).toBeGreaterThan(Number(parts[1] ?? 0))
    }
  })
})

describe('the Workflow surface', () => {
  it('calls only methods the wire declares, so a UI-only write cannot appear', () => {
    const known = wireMethods()
    expect(known.size).toBeGreaterThan(0)

    for (const name of ['faces/WorkflowFace.tsx', 'faces/workflow.ts']) {
      for (const tag of calledMethods(source(name))) {
        expect(known, `${name} calls ${tag}`).toContain(tag)
      }
    }
  })

  it('reads the open sprint and its batch, and writes only the open act and the lanes', () => {
    const called = [...new Set(calledMethods(source('faces/WorkflowFace.tsx')))].sort()

    expect(called).toEqual([
      'lane.add',
      'lane.delete',
      'lane.list',
      'lane.rename',
      'sprint.list',
      'sprint.open',
      'sprint.show',
    ])
    expect(calledMethods(source('faces/workflow.ts'))).toEqual([])
  })

  it('is reachable from the shell tab strip', () => {
    expect(source('App.tsx')).toContain('WorkflowFace')
    expect(source('shell/tabs.ts')).toContain('workflow')
  })
})
