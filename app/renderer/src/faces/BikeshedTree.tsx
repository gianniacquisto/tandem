import { useState, type DragEvent } from 'react'

import type { BikeshedHealth } from '../wire/BikeshedHealth'
import type { NoteNodeView } from '../wire/NoteNodeView'
import {
  canReparent,
  flattenTree,
  healthSummary,
  isHiddenPath,
} from '../shell/bikeshed'

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
}: BikeshedTreeProps) {
  const rows = flattenTree(nodes, expanded)
  const [dragging, setDragging] = useState<string | null>(null)
  const [over, setOver] = useState<string | null>(null)

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

      {nodes.length === 0 ? (
        <p className="tree__empty">
          The bikeshed is empty. Notes are plain markdown files an agent or the
          editor can create.
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
