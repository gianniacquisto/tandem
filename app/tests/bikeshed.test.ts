import { describe, expect, it } from 'vitest'

import type { NoteNodeView } from '../renderer/src/wire/NoteNodeView'
import {
  canReparent,
  flattenTree,
  focusNodes,
  recoverableLabel,
  ticketHref,
} from '../renderer/src/shell/bikeshed'

const FOLDERS: NoteNodeView[] = [
  {
    name: 'daily',
    path: 'daily',
    kind: 'folder',
    children: [
      { name: '2026-10-30', path: 'daily/2026-10-30.md', kind: 'note', children: [] },
    ],
  },
  {
    name: 'projects',
    path: 'projects',
    kind: 'folder',
    children: [
      {
        name: 'fixture',
        path: 'projects/fixture',
        kind: 'folder',
        children: [
          {
            name: 'fixture',
            path: 'projects/fixture/fixture.md',
            kind: 'note',
            children: [],
          },
        ],
      },
    ],
  },
  { name: 'Index', path: 'Index.md', kind: 'note', children: [] },
]

describe('the tree rows', () => {
  it('draws a folder collapsed and its children when expanded', () => {
    const collapsed = flattenTree(FOLDERS, new Set())
    expect(collapsed.map((row) => row.node.path)).toEqual(['daily', 'projects', 'Index.md'])
    expect(collapsed.map((row) => row.depth)).toEqual([0, 0, 0])

    const expanded = flattenTree(FOLDERS, new Set(['projects', 'projects/fixture']))
    expect(expanded.map((row) => row.node.path)).toEqual([
      'daily',
      'projects',
      'projects/fixture',
      'projects/fixture/fixture.md',
      'Index.md',
    ])
    expect(expanded.map((row) => row.depth)).toEqual([0, 0, 1, 2, 0])
  })

  it('opens the folders an open note lives under', () => {
    const opened = flattenTree(FOLDERS, new Set(['projects', 'projects/fixture']))
    expect(opened.some((row) => row.node.path === 'projects/fixture/fixture.md')).toBe(true)
  })
})

describe('the drag that re-parents', () => {
  it('refuses a folder into its own descendant', () => {
    expect(canReparent('projects', 'projects/fixture')).toBe(false)
  })

  it('refuses a note dropped on a note', () => {
    expect(canReparent('Index.md', 'daily/2026-10-30.md')).toBe(false)
  })

  it('moves a note into a sibling folder and into the root', () => {
    expect(canReparent('Index.md', 'daily')).toBe(true)
    expect(canReparent('daily/2026-10-30.md', '')).toBe(true)
  })

  it('refuses the folder the note already sits in only when it would be a no-op', () => {
    expect(canReparent('daily/2026-10-30.md', 'daily')).toBe(false)
  })
})

describe('the project focus', () => {
  it('draws the whole vault when nothing is focused: the show-all escape', () => {
    expect(focusNodes(FOLDERS, null)).toBe(FOLDERS)
  })

  it('draws the focused project folder alone, matched case-insensitively', () => {
    const focused = focusNodes(FOLDERS, 'Fixture')
    expect(focused.map((node) => node.path)).toEqual(['daily', 'projects', 'Index.md'])
    expect(focused[1]?.children.map((child) => child.path)).toEqual(['projects/fixture'])
  })

  it('falls back to the whole vault when no project folder matches, so a stale focus hides nothing', () => {
    expect(focusNodes(FOLDERS, 'Retro')).toBe(FOLDERS)
  })
})

describe('a recovery', () => {
  it('names the draft in human words, never the tmp path or byte count', () => {
    const label = recoverableLabel({
      file: '.note-0000-0000-0000-000000000001.tmp',
      bytes: 42,
      modifiedAt: '2026-10-30T12:00:00Z',
    })
    expect(label.toLowerCase()).toContain('draft')
    expect(label.toLowerCase()).toContain('interrupted save')
    expect(label).not.toContain('.note-')
    expect(label).not.toContain('.tmp')
    expect(label).not.toContain('42')
    expect(label.toLowerCase()).not.toContain('bytes')
    expect(label.toLowerCase()).not.toContain('error')
  })

  it('names the folder a nested temp was kept in, not the tmp file', () => {
    const label = recoverableLabel({
      file: 'daily/.note-0000-0000-0000-000000000001.tmp',
      bytes: 42,
      modifiedAt: '2026-10-30T12:00:00Z',
    })
    expect(label).toContain('daily')
    expect(label).not.toContain('.note-')
    expect(label).not.toContain('.tmp')
  })
})

describe('the ticket sheet link', () => {
  it('opens the tickets face at the ticket it names', () => {
    expect(ticketHref('C68DA7C7-AA50-4673-A68B-D067C5A5E7DD')).toBe(
      '#/tickets?ticket=C68DA7C7-AA50-4673-A68B-D067C5A5E7DD',
    )
  })
})
