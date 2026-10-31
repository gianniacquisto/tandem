import { describe, expect, it } from 'vitest'

import type { NoteNodeView } from '../renderer/src/wire/NoteNodeView'
import {
  canReparent,
  flattenTree,
  healthSummary,
  isHiddenPath,
  recoverableLabel,
  resolveWikilink,
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

describe('the hidden layer', () => {
  it('is any path component that starts with a dot', () => {
    expect(isHiddenPath('.raw/capture.md')).toBe(true)
    expect(isHiddenPath('projects/.draft/note.md')).toBe(true)
    expect(isHiddenPath('projects/fixture/fixture.md')).toBe(false)
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

describe('a wikilink click', () => {
  const paths = ['Index.md', 'daily/2026-10-30.md', 'projects/fixture/fixture.md']

  it('resolves a unique basename to its path', () => {
    expect(resolveWikilink('fixture', paths)).toBe('projects/fixture/fixture.md')
  })

  it('resolves a path as written', () => {
    expect(resolveWikilink('projects/fixture/fixture', paths)).toBe(
      'projects/fixture/fixture.md',
    )
  })

  it('leaves a duplicate basename unresolved', () => {
    expect(resolveWikilink('fixture', [...paths, 'projects/other/fixture.md'])).toBeNull()
  })

  it('leaves a target that resolves to nothing unresolved', () => {
    expect(resolveWikilink('missing', paths)).toBeNull()
  })
})

describe('the health panel summary', () => {
  it('names the broken links and the orphans in the Swift panel words', () => {
    expect(
      healthSummary({
        broken: [{ note: 'Index.md', link: 'missing', line: 4 }],
        orphans: ['daily/2026-10-30.md'],
        counts: { notes: 3, links: 2, broken: 1, orphans: 1 },
      }),
    ).toBe('1 broken, 1 orphan')
  })

  it('says the clean report in one line', () => {
    expect(
      healthSummary({
        broken: [],
        orphans: [],
        counts: { notes: 3, links: 1, broken: 0, orphans: 0 },
      }),
    ).toBe('No broken links, no orphan notes.')
  })
})

describe('a recovery', () => {
  it('names the temp as a draft, never as an error', () => {
    const label = recoverableLabel({
      file: '.note-0000-0000-0000-000000000001.tmp',
      bytes: 42,
      modifiedAt: '2026-10-30T12:00:00Z',
    })
    expect(label).toContain('draft')
    expect(label).toContain('.note-0000-0000-0000-000000000001.tmp')
    expect(label.toLowerCase()).not.toContain('error')
  })
})

describe('the ticket sheet link', () => {
  it('opens the tickets face at the ticket it names', () => {
    expect(ticketHref('C68DA7C7-AA50-4673-A68B-D067C5A5E7DD')).toBe(
      '#/tickets?ticket=C68DA7C7-AA50-4673-A68B-D067C5A5E7DD',
    )
  })
})
