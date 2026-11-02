import type { Scope } from './board'

/**
 * The keyboard set the pruned palette and the two tabs took with them
 * (`MainView.swift`, `TandemShortcuts` / `TicketsShortcuts`), as a pure reading
 * of one key event.
 *
 * The face owns no global shortcut: this answers what one event means and the
 * face decides whether it can act. A field keeps its keys: only Escape is read
 * out of one, because clearing a search is what Escape means there.
 */

export type KeyInput = {
  key: string
  metaKey: boolean
  ctrlKey: boolean
  altKey: boolean
  shiftKey: boolean
  /** The event came from an input, a select, a textarea or a contenteditable. */
  editable: boolean
}

export type KeyIntent =
  | { type: 'focus-quick-add' }
  | { type: 'focus-search' }
  | { type: 'scope'; scope: Scope }
  | { type: 'select'; axis: 'row' | 'column'; delta: -1 | 1 }
  | { type: 'move-in-lane'; delta: -1 | 1 }
  | { type: 'move-across'; delta: -1 | 1 }
  | { type: 'open' }
  | { type: 'clear-search' }
  | { type: 'dismiss' }

const SCOPES: Record<string, Scope> = { '1': 'open', '2': 'done', '3': 'all' }

/**
 * The one modifier this face treats as "mod" (`keyIntent` reads meta or ctrl as
 * the same key), as the platform's keyboard spells it. A hint that printed
 * `⌘` on Windows would name a key the keyboard does not have, and the glyph is
 * missing from the usual Windows UI fonts besides.
 */
export function modifierLabel(platform: string = navigator.platform): string {
  return platform.startsWith('Mac') ? '⌘' : 'Ctrl+'
}

export function keyIntent(input: KeyInput): KeyIntent | null {
  const mod = input.metaKey || input.ctrlKey
  if (mod) {
    const scope = SCOPES[input.key]
    if (scope !== undefined) return { type: 'scope', scope }
    if (input.key === 'n') return { type: 'focus-quick-add' }
    if (input.key === 'f') return { type: 'focus-search' }
    return null
  }

  // A field keeps its own keys: an arrow, Return or a typed letter belongs to
  // the field, and only Escape is read out of one.
  if (input.editable) {
    return input.key === 'Escape' ? { type: 'clear-search' } : null
  }

  if (input.altKey) {
    switch (input.key) {
      case 'ArrowUp':
        return { type: 'move-in-lane', delta: -1 }
      case 'ArrowDown':
        return { type: 'move-in-lane', delta: 1 }
      case 'ArrowLeft':
        return { type: 'move-across', delta: -1 }
      case 'ArrowRight':
        return { type: 'move-across', delta: 1 }
      default:
        return null
    }
  }

  switch (input.key) {
    case 'ArrowDown':
      return { type: 'select', axis: 'row', delta: 1 }
    case 'ArrowUp':
      return { type: 'select', axis: 'row', delta: -1 }
    case 'ArrowRight':
      return { type: 'select', axis: 'column', delta: 1 }
    case 'ArrowLeft':
      return { type: 'select', axis: 'column', delta: -1 }
    case 'Enter':
      return { type: 'open' }
    case 'Escape':
      return { type: 'dismiss' }
    default:
      return null
  }
}
