import { useState, type DragEvent, type FormEvent, type ReactNode } from 'react'

import type { BikeshedHealth } from '../wire/BikeshedHealth'
import type { NoteNodeView } from '../wire/NoteNodeView'
import {
  canReparent,
  flattenTree,
  healthSummary,
  isHiddenPath,
} from '../shell/bikeshed'
import { defaultCreateFolder, resolveFolderPath, resolveNotePath } from '../shell/note'

export type BikeshedTreeProps = {
  nodes: NoteNodeView[]
  expanded: Set<string>
  hidden: boolean
  openPath: string | null
  health: BikeshedHealth | null
  onToggle: (path: string) => void
  onReveal: (hidden: boolean) => void
  onOpen: (path: string) => void
  onMove: (source: string, into: string) => void
  onCreateNote: (path: string) => void
  onCreateFolder: (path: string) => void
  onDaily: () => void
}

function name(path: string): string {
  const base = path.split('/').pop() ?? path
  return base.replace(/\.md$/i, '')
}

/**
 * The tree pane and the Health panel under it (`BikeTreeView` and
 * `BikeHealthPanel` in the Swift face).
 *
 * A note or a folder is a drag source; a folder row and the vault's own row are
 * drop targets. The gesture asks the core for the same move the `note move`
 * verb performs, and `canReparent` keeps it from asking for one the domain
 * refuses. Revealed hidden rows are drawn dimmed, as in the Swift tree.
 */
export function BikeshedTree({
  nodes,
  expanded,
  hidden,
  openPath,
  health,
  onToggle,
  onReveal,
  onOpen,
  onMove,
  onCreateNote,
  onCreateFolder,
  onDaily,
}: BikeshedTreeProps) {
  const rows = flattenTree(nodes, expanded)
  const [dragging, setDragging] = useState<string | null>(null)
  const [over, setOver] = useState<string | null>(null)
  const [creating, setCreating] = useState<'note' | 'folder' | null>(null)
  const [draft, setDraft] = useState('')
  const folder = defaultCreateFolder(openPath)
  const where = folder === '' ? 'Bikeshed' : folder

  function drop(into: string): void {
    if (dragging === null) return
    if (canReparent(dragging, into)) onMove(dragging, into)
    setDragging(null)
    setOver(null)
  }

  function target(path: string) {
    return {
      onDragOver: (event: DragEvent) => {
        if (dragging === null || !canReparent(dragging, path)) return
        event.preventDefault()
        setOver(path)
      },
      onDragLeave: () => setOver(null),
      onDrop: (event: DragEvent) => {
        event.preventDefault()
        drop(path)
      },
    }
  }

  function start(kind: 'note' | 'folder'): void {
    setCreating(kind)
    setDraft('')
  }

  function cancel(): void {
    setCreating(null)
    setDraft('')
  }

  function submit(event: FormEvent): void {
    event.preventDefault()
    if (creating === null) return
    const trimmed = draft.trim()
    if (trimmed === '') return
    const path =
      creating === 'note' ? resolveNotePath(folder, trimmed) : resolveFolderPath(folder, trimmed)
    if (creating === 'note') onCreateNote(path)
    else onCreateFolder(path)
    setCreating(null)
    setDraft('')
  }

  function actions(): ReactNode {
    return (
      <div className="tree__actions">
        <button type="button" className="tree__action" onClick={onDaily}>
          Today&apos;s note
        </button>
        <button
          type="button"
          className="tree__new tree__new--note"
          onClick={() => start('note')}
        >
          New note
        </button>
        <button
          type="button"
          className="tree__new tree__new--folder"
          onClick={() => start('folder')}
        >
          New folder
        </button>
      </div>
    )
  }

  function prompt(): ReactNode {
    if (creating === null) return null
    const label = creating === 'note' ? 'New note' : 'New folder'
    return (
      <form
        className="tree__prompt"
        role="dialog"
        aria-label={label}
        onSubmit={submit}
        onKeyDown={(event) => {
          if (event.key === 'Escape') cancel()
        }}
      >
        <label className="tree__prompt-label" htmlFor="tree-prompt-name">
          {label} in {where}
        </label>
        <input
          id="tree-prompt-name"
          className="tree__prompt-input"
          autoFocus
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder={creating === 'note' ? 'note name' : 'folder name'}
          aria-label="Name"
        />
        <div className="tree__prompt-row">
          <button
            type="submit"
            className="tree__new tree__prompt-create"
            disabled={draft.trim() === ''}
          >
            Create
          </button>
          <button type="button" className="tree__new" onClick={cancel}>
            Cancel
          </button>
        </div>
      </form>
    )
  }

  return (
    <aside className="bikeshed__tree" aria-label="Bikeshed notes">
      <div
        className="tree__vault"
        data-over={over === '' ? 'true' : undefined}
        {...target('')}
      >
        <span className="tree__vault-name">Bikeshed</span>
        <label className="tree__reveal">
          <input
            type="checkbox"
            checked={hidden}
            onChange={(event) => onReveal(event.target.checked)}
          />
          Show hidden
        </label>
      </div>
      {actions()}
      {prompt()}

      {nodes.length === 0 ? (
        <p className="tree__empty">
          The bikeshed is empty. Create a note or a folder to get started.
        </p>
      ) : (
        <ul className="tree" role="tree">
          {rows.map(({ node, depth }) => {
            const folder = node.kind === 'folder'
            const revealed = hidden && isHiddenPath(node.path)
            return (
              <li key={node.path} role="none">
                <div
                  className="tree__row"
                  role="treeitem"
                  aria-level={depth + 1}
                  aria-expanded={folder ? expanded.has(node.path) : undefined}
                  aria-selected={node.path === openPath}
                  data-kind={node.kind}
                  data-hidden={revealed ? 'true' : undefined}
                  data-over={over === node.path ? 'true' : undefined}
                  draggable
                  onDragStart={(event) => {
                    event.dataTransfer.setData('text/plain', node.path)
                    event.dataTransfer.effectAllowed = 'move'
                    setDragging(node.path)
                  }}
                  onDragEnd={() => {
                    setDragging(null)
                    setOver(null)
                  }}
                  {...(folder ? target(node.path) : {})}
                >
                  <button
                    type="button"
                    className="tree__label"
                    style={{ paddingLeft: `${8 + depth * 14}px` }}
                    onClick={() => (folder ? onToggle(node.path) : onOpen(node.path))}
                    title={revealed ? 'Hidden layer: outside search, backlinks and wikilinks.' : node.path}
                  >
                    <span className="tree__twisty" aria-hidden="true">
                      {folder ? (expanded.has(node.path) ? '\u25be' : '\u25b8') : ''}
                    </span>
                    <span className="tree__name">{name(node.path)}</span>
                  </button>
                </div>
              </li>
            )
          })}
        </ul>
      )}

      {health === null ? null : (
        <section className="health" aria-label="Health">
          <header className="health__header">
            <h2 className="health__title">Health</h2>
            <span className="health__count">{healthSummary(health)}</span>
          </header>
          {health.broken.length === 0 && health.orphans.length === 0 ? (
            <p className="health__clean">No broken links, no orphan notes.</p>
          ) : (
            <ul className="health__rows">
              {health.broken.map((link) => (
                <li key={`${link.note}:${link.line}:${link.link}`}>
                  <button
                    type="button"
                    className="health__row"
                    onClick={() => onOpen(link.note)}
                  >
                    <span className="health__row-name">{name(link.note)}</span>
                    <span className="health__row-detail">
                      {'\u2192'} {link.link} {'\u00b7'} line {link.line}
                    </span>
                  </button>
                </li>
              ))}
              {health.orphans.map((path) => (
                <li key={path}>
                  <button type="button" className="health__row" onClick={() => onOpen(path)}>
                    <span className="health__row-name">{name(path)}</span>
                    <span className="health__row-detail">nothing links here</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </aside>
  )
}
