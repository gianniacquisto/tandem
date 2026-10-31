import type { BikeshedHealth } from '../wire/BikeshedHealth'
import type { NoteNodeView } from '../wire/NoteNodeView'
import type { RecoveryView } from '../wire/RecoveryView'

/**
 * The Bikeshed face's pure decisions: how the tree flattens, what a drag may
 * move, which note a wikilink names, and the words the panels use.
 *
 * The core owns every rule here (the tree, the move, wikilink resolution); this
 * module adapts them to the drawing and nothing more. React stays in the face.
 */
export type TreeRow = {
  node: NoteNodeView
  depth: number
}

/** The rows of the tree: a folder draws its children only while expanded. */
export function flattenTree(nodes: NoteNodeView[], expanded: Set<string>): TreeRow[] {
  const rows: TreeRow[] = []
  function walk(level: NoteNodeView[], depth: number): void {
    for (const node of level) {
      rows.push({ node, depth })
      if (node.kind === 'folder' && expanded.has(node.path)) walk(node.children, depth + 1)
    }
  }
  walk(nodes, 0)
  return rows
}

/** Every folder path in a tree, for the one-time expansion of the scaffold. */
export function folderPaths(nodes: NoteNodeView[]): string[] {
  return nodes.flatMap((node) =>
    node.kind === 'folder' ? [node.path, ...folderPaths(node.children)] : [],
  )
}

/** Whether a path names something in the hidden layer. */
export function isHiddenPath(path: string): boolean {
  return path.split('/').some((component) => component.startsWith('.'))
}

/** The folders a path lives under, root first. */
export function ancestorsOf(path: string): string[] {
  const parts = path.split('/')
  return parts.slice(0, Math.max(0, parts.length - 1)).map((_, index) => parts.slice(0, index + 1).join('/'))
}

function parentOf(path: string): string {
  const slash = path.lastIndexOf('/')
  return slash === -1 ? '' : path.slice(0, slash)
}

function isNotePath(path: string): boolean {
  return path.toLowerCase().endsWith('.md')
}

/**
 * Whether `source` may move into `into`: the core's own rules, so a gesture
 * never asks for a move the domain will refuse.
 *
 * A folder cannot enter itself or its own descendant, a note cannot land on a
 * note, and a move into the folder it already sits in is a no-op the gesture
 * does not offer.
 */
export function canReparent(source: string, into: string): boolean {
  if (source === '' || source === into) return false
  if (into !== '' && isNotePath(into)) return false
  if (into === source || into.startsWith(`${source}/`)) return false
  if (parentOf(source) === into) return false
  return true
}

function stripDecoration(target: string): string {
  const bare = target.split('#')[0]?.split('|')[0]?.trim() ?? ''
  return bare.startsWith('./') ? bare.slice(2) : bare
}

/**
 * The path a wikilink target names, or null when it is broken.
 *
 * The core's rule, copied only as the drawing's router: exactly one match,
 * case-insensitive, with or without `.md`; a target carrying a slash is a
 * qualified path, a bare target a basename. The expansion menu resolves through
 * the core, so this is never the only truth about a link.
 */
export function resolveWikilink(target: string, paths: string[]): string | null {
  const bare = stripDecoration(target)
  if (bare === '') return null
  const lowered = bare.toLowerCase().replace(/\.md$/, '')
  const qualified = bare.includes('/')
  const matches = paths.filter((path) => {
    const lower = path.toLowerCase()
    if (qualified) return lower.replace(/\.md$/, '') === lowered
    const base = lower.split('/').pop()?.replace(/\.md$/, '') ?? ''
    return base === lowered
  })
  return matches.length === 1 ? (matches[0] ?? null) : null
}

/** The Health panel's one-line summary, in the Swift panel's words. */
export function healthSummary(health: BikeshedHealth): string {
  const parts: string[] = []
  if (health.counts.broken > 0) parts.push(`${health.counts.broken} broken`)
  if (health.counts.orphans > 0) parts.push(`${health.counts.orphans} orphan`)
  return parts.length === 0 ? 'No broken links, no orphan notes.' : parts.join(', ')
}

/** A recovery named as what it is: a draft from an interrupted save. */
export function recoverableLabel(recovery: RecoveryView): string {
  const bytes = `${recovery.bytes} bytes`
  return `A draft from an interrupted save: ${recovery.file} (${bytes})`
}

/** The Tickets face at the sheet a note links to (PRT-17 reads the query). */
export function ticketHref(id: string): string {
  return `#/tickets?ticket=${encodeURIComponent(id)}`
}

/** Every note path in a tree, for the bundle's flat wikilink index. */
export function notePaths(nodes: NoteNodeView[]): string[] {
  return nodes.flatMap((node) =>
    node.kind === 'note' ? [node.path] : notePaths(node.children),
  )
}
