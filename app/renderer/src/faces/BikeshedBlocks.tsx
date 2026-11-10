import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ClipboardEvent as ReactClipboardEvent,
} from 'react'

import { serialiseDocument, type Document } from '../shell/blocks'
import type { BlockPoint } from '../shell/caret'
import { assetUrl, resolveImageTarget } from '../shell/image'
import {
  applyShortcut,
  backspace,
  candidateText,
  deleteForward,
  deleteSelection,
  fromDocument,
  insertText,
  moveDown,
  moveLeft,
  moveRight,
  moveUp,
  paste,
  positionAtSource,
  record,
  redo,
  rowCount,
  rowText,
  selectAll,
  selectedText,
  sourceOffset,
  split,
  toDocument,
  toggleStyle,
  toggleTask,
  undo,
  withCaret,
  withSelection,
  emptyHistory,
  type EditorState,
  type History,
  type Position,
  type Selection,
  type SurfaceBlock,
} from '../shell/editor-ops'
import { inlineKey } from '../shell/inline-commands'
import { filterNotes, parseInline, wikilinkQueryAt, type NoteChoice } from '../shell/inline'
import { InlineRuns, WikilinkPicker } from './InlineRuns'

export type BikeshedBlocksProps = {
  /** UI-17's document: the surface's one truth. */
  document: Document
  /** Changes when the note is opened or adopted, so the host takes focus. */
  focusToken: number
  /** The persisted note font size; Write keeps the same reading size. */
  fontSize: number
  /** The containing note's vault path: image targets resolve against its folder. */
  notePath: string
  /** The caret a mode switch carried, as a block point; null for none. */
  caret?: BlockPoint | null
  onChange: (next: Document) => void
  /** Reports the surface's caret so a switch can carry it. */
  onCaret?: (point: BlockPoint | null) => void
  onSendToTodos: (text: string) => void
  /** The read-only card's pointer: switch the face to Source at that block. */
  onSwitchToSource: (block: number) => void
  /** Every note path in the face's index, for chips and the `[[` picker. */
  noteIndex?: string[]
  /** Open a resolved note; the chip and a bikeshed link call it. */
  onOpenNote?: (path: string) => void
}

type PickerState = {
  block: number
  row: number
  /** The offset of the `[[` that opened the picker. */
  from: number
  query: string
  active: number
  left: number
  top: number
}

/**
 * The Write surface: one contenteditable host over UI-17's blocks.
 *
 * The block model is the document and the DOM is its view. Typing is left to
 * the browser inside an edited block and read back on `input`; structure
 * (Enter, Backspace across blocks, a shortcut) is applied through the pure
 * `editor-ops` module and the DOM is re-rendered from the result. A typed
 * keystroke never goes through React's state: the live model is a ref, so the
 * host is not re-rendered under the caret. IME composition is ignored until
 * `compositionend`, so autosave cannot see half-composed text.
 */
export function BikeshedBlocks({
  document: doc,
  focusToken,
  fontSize,
  notePath,
  caret,
  onChange,
  onCaret,
  onSendToTodos,
  onSwitchToSource,
  noteIndex = [],
  onOpenNote,
}: BikeshedBlocksProps) {
  const [state, setState] = useState<EditorState>(() => fromDocument(doc))
  const [picker, setPicker] = useState<PickerState | null>(null)
  // The live model. React's `state` is the rendered structure; typing updates
  // this ref alone, so a keystroke never re-renders under the caret.
  const stateRef = useRef(state)
  const historyRef = useRef<History>(emptyHistory)
  const host = useRef<HTMLDivElement | null>(null)
  const composing = useRef(false)
  const pending = useRef<Selection | null>(null)
  const lastText = useRef(serialiseDocument(doc))
  // The switch's caret, read once when this surface takes focus: the face
  // passes a fresh object each render, and re-placing it would fight typing.
  const incoming = useRef<BlockPoint | null>(caret ?? null)
  incoming.current = caret ?? null

  // An external document (open, adopt, Take theirs) replaces the surface; the
  // face's own echo of our change serialises to the same bytes and is ignored.
  useEffect(() => {
    const incoming = serialiseDocument(doc)
    if (incoming === lastText.current) return
    lastText.current = incoming
    const next = fromDocument(doc)
    stateRef.current = next
    setState(next)
    historyRef.current = emptyHistory
  }, [doc])

  // A typed keystroke updates the live model without a render, so React never
  // rewrites the text node under the caret. Structure goes through setState.
  const apply = useCallback(
    (next: EditorState, merge = false) => {
      const before = stateRef.current
      if (next === before) return
      historyRef.current = record(historyRef.current, before, { merge })
      stateRef.current = next
      lastText.current = serialiseDocument(toDocument(next))
      setState(next)
      pending.current = next.selection
      onChange(toDocument(next))
      reportCaret(next.selection.head)
    },
    // `reportCaret` reads the live refs; only the prop is a dependency.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [onChange, onCaret],
  )

  /** Report the caret as a block point, the unit the switch carries. */
  function reportCaret(at?: Position): void {
    if (onCaret === undefined) return
    const target = host.current
    const selection = target === null ? null : readSelection(target)
    const state = stateRef.current
    const position = at ?? selection?.head ?? state.selection.head
    const surface = state.blocks[position.block]
    onCaret(surface === undefined ? null : { block: position.block, offset: sourceOffset(surface, position) })
  }

  useLayoutEffect(() => {
    if (typeof window === 'undefined') return
    const target = host.current
    if (target === null) return
    // A typing keystroke never re-renders, so React's view of an edited text
    // node can lag behind the DOM. A structural edit re-renders: bring every
    // row back to the model before the caret is placed, or the old token
    // survives (the `# ` in a shortcut is the case that found this).
    syncDomText(target, stateRef.current)
    const selection = pending.current
    if (selection === null) return
    pending.current = null
    target.focus()
    setDomSelection(target, selection)
  })

  useEffect(() => {
    if (typeof window === 'undefined') return
    const target = host.current
    if (target === null) return
    target.focus()
    // The switch's offset wins when one arrived; otherwise the surface focuses
    // the note's own caret (the empty note's first block after a creation).
    const point = incoming.current
    const surface = point === null ? undefined : stateRef.current.blocks[point.block]
    if (point !== null && surface !== undefined) {
      const position = positionAtSource(surface, point.block, point.offset)
      const next = withCaret(stateRef.current, position.block, position.row, position.offset)
      stateRef.current = next
      setState(next)
      pending.current = next.selection
      setDomSelection(target, next.selection)
      return
    }
    setDomSelection(target, stateRef.current.selection)
    // `focusToken` is the document revision this focus rule follows.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusToken])

  // A mouse click or an arrow move changes which run holds the caret, and the
  // run's markers must appear. The model learns the selection and a render
  // draws the reveal; typing itself still updates the ref alone.
  useEffect(() => {
    if (typeof window === 'undefined') return
    function onSelectionChange(): void {
      const target = host.current
      if (target === null) return
      // The code language field and the task checkbox are not the note: a
      // selection change inside them must not re-render the surface under
      // their value.
      const active = document.activeElement
      if (active !== null && active.closest('input, textarea, select, button') !== null) return
      const selection = readSelection(target)
      if (selection === null) return
      const current = stateRef.current
      if (same(selection.anchor, current.selection.anchor) && same(selection.head, current.selection.head)) {
        return
      }
      const next = withSelection(current, selection.anchor, selection.head)
      stateRef.current = next
      const surface = next.blocks[next.selection.head.block]
      if (surface === undefined || !isTextSurface(surface)) return
      if (revealKey(current.selection) === revealKey(next.selection)) return
      pending.current = next.selection
      setState({ ...next })
    }
    document.addEventListener('selectionchange', onSelectionChange)
    return () => document.removeEventListener('selectionchange', onSelectionChange)
  }, [])

  function caretPoint(target: HTMLElement): { left: number; top: number } {
    if (typeof window === 'undefined') return { left: 0, top: 0 }
    const selection = window.getSelection()
    if (selection !== null && selection.rangeCount > 0) {
      const rect = selection.getRangeAt(0).getBoundingClientRect()
      if (rect.width !== 0 || rect.height !== 0) {
        return { left: rect.left, top: rect.bottom + 4 }
      }
    }
    const row = target.querySelector('[data-block][data-row]')
    if (row !== null) {
      const rect = row.getBoundingClientRect()
      return { left: rect.left, top: rect.bottom + 4 }
    }
    const rect = target.getBoundingClientRect()
    return { left: rect.left, top: rect.top }
  }

  function pickerFor(value: EditorState, point: { left: number; top: number }): PickerState | null {
    const head = value.selection.head
    const surface = value.blocks[head.block]
    if (surface === undefined || !isTextSurface(surface)) return null
    const trigger = wikilinkQueryAt(rowText(surface, head.row), head.offset)
    if (trigger === null) return null
    return {
      block: head.block,
      row: head.row,
      from: trigger.from,
      query: trigger.query,
      active: picker?.block === head.block && picker.row === head.row && picker.from === trigger.from ? picker.active : 0,
      left: point.left,
      top: point.top,
    }
  }

  function refreshPicker(next: EditorState): void {
    const target = host.current
    if (target === null) return
    const found = pickerFor(next, caretPoint(target))
    setPicker(found)
    if (found !== null) {
      // Opening the picker is a structural render: the model that was typed
      // must be the one React renders, or the DOM returns to the old text.
      pending.current = next.selection
      setState({ ...next })
    }
  }

  function choose(choice: NoteChoice): void {
    if (picker === null) return
    const current = stateRef.current
    const surface = current.blocks[picker.block]
    if (surface === undefined || !isTextSurface(surface)) return
    const text = rowText(surface, picker.row)
    const before = text.slice(0, picker.from)
    const after = text.slice(picker.from + 2 + picker.query.length)
    const inserted = `${before}[[${choice.insert}]]${after}`
    const caret = before.length + 2 + choice.insert.length + 2
    const rows = surface.rows.map((row, index) =>
      index === picker.row ? { ...row, text: inserted } : row,
    )
    const next: EditorState = {
      ...current,
      blocks: current.blocks.map((block, index) =>
        index === picker.block ? { ...surface, rows, edited: true } : block,
      ),
      selection: {
        anchor: { block: picker.block, row: picker.row, offset: caret },
        head: { block: picker.block, row: picker.row, offset: caret },
      },
    }
    setPicker(null)
    apply(next, false)
  }

  function handleInput(): void {
    const target = host.current
    if (target === null || composing.current) return
    const before = stateRef.current
    const next = readFromDom(target, before)
    const selection = readSelection(target)
    const base = selection === null ? next : withSelection(next, selection.anchor, selection.head)
    const converted = applyShortcut(base, 'space')
    if (converted !== base) {
      apply(converted, false)
      refreshPicker(converted)
      return
    }
    stateRef.current = base
    historyRef.current = record(historyRef.current, before, { merge: true })
    const markdown = serialiseDocument(toDocument(base))
    lastText.current = markdown
    onChange(toDocument(base))
    reportCaret(base.selection.head)
    // Plain prose keeps UI-18's no-render typing path. Once a construct is in
    // the row, a render is what draws it, so the model that was typed is handed
    // back with the caret restored.
    if (!plainRow(base)) {
      pending.current = base.selection
      setState({ ...base })
    }
    refreshPicker(base)
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLDivElement>): void {
    if (composing.current || event.nativeEvent.isComposing) return
    // The code language field and the task checkbox are not the note: their
    // keys never reach the caret model.
    const origin = event.target as HTMLElement
    if (origin.closest('input, textarea, select, button') !== null) return
    const target = host.current
    if (target === null) return
    const mod = event.metaKey || event.ctrlKey
    const key = event.key.toLowerCase()

    if (picker !== null) {
      if (event.key === 'Escape') {
        event.preventDefault()
        setPicker(null)
        return
      }
      const choices = filterNotes(noteIndex, picker.query)
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault()
        if (choices.length === 0) return
        const delta = event.key === 'ArrowDown' ? 1 : -1
        setPicker({
          ...picker,
          active: (picker.active + delta + choices.length) % choices.length,
        })
        return
      }
      if (event.key === 'Enter') {
        event.preventDefault()
        const choice = choices[picker.active]
        if (choice !== undefined) choose(choice)
        return
      }
    }

    if (mod && !event.altKey && key === 'z') {
      event.preventDefault()
      const result = event.shiftKey
        ? redo(historyRef.current, stateRef.current)
        : undo(historyRef.current, stateRef.current)
      historyRef.current = result.history
      stateRef.current = result.state
      setState(result.state)
      pending.current = result.state.selection
      lastText.current = serialiseDocument(toDocument(result.state))
      onChange(toDocument(result.state))
      return
    }
    if (mod && event.shiftKey && key === 't') {
      event.preventDefault()
      const current = readSelection(target)
      const selected =
        current === null
          ? stateRef.current
          : withSelection(stateRef.current, current.anchor, current.head)
      onSendToTodos(candidateText(selected))
      return
    }
    if (mod && !event.shiftKey && key === 'a') {
      event.preventDefault()
      const all = selectAll(stateRef.current)
      stateRef.current = all
      setDomSelection(target, all.selection)
      return
    }

    const selection = readSelection(target)
    const base = selection === null ? stateRef.current : withSelection(stateRef.current, selection.anchor, selection.head)

    const inline = inlineKey({ key, mod, shift: event.shiftKey, alt: event.altKey })
    if (inline !== null) {
      event.preventDefault()
      apply(toggleStyle(base, inline), false)
      return
    }

    if (event.key === 'Enter') {
      event.preventDefault()
      if (event.shiftKey) {
        apply(insertText(base, '\n'), false)
        return
      }
      const converted = applyShortcut(base, 'enter')
      if (converted !== base) {
        apply(converted, false)
        return
      }
      apply(split(base), false)
      return
    }

    if (event.key === 'Backspace' || event.key === 'Delete') {
      if (selection !== null && !same(selection.anchor, selection.head)) {
        event.preventDefault()
        apply(deleteSelection(base), false)
        return
      }
      const head = base.selection.head
      const surface = base.blocks[head.block]
      if (surface === undefined) return
      const length = rowText(surface, head.row).length
      const atBoundary =
        event.key === 'Backspace'
          ? head.offset === 0
          : head.offset === length && head.row === rowCount(surface) - 1
      if (!atBoundary) return
      event.preventDefault()
      const next = event.key === 'Backspace' ? backspace(base) : deleteForward(base)
      apply(next, false)
      refreshPicker(next)
      return
    }

    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight' || event.key === 'ArrowUp' || event.key === 'ArrowDown') {
      // A shifted arrow is the surface's: the model owns the anchor and the
      // head, so a selection crosses a block boundary the same way on every
      // platform (Chromium's native block extension is not).
      if (event.shiftKey) {
        event.preventDefault()
        const moved =
          event.key === 'ArrowLeft'
            ? moveLeft(base, true)
            : event.key === 'ArrowRight'
              ? moveRight(base, true)
              : event.key === 'ArrowUp'
                ? moveUp(base, true)
                : moveDown(base, true)
        if (moved !== base) apply(moved, false)
        return
      }
      if (selection !== null && !same(selection.anchor, selection.head)) return
      const head = base.selection.head
      const surface = base.blocks[head.block]
      if (surface === undefined) return
      const length = rowText(surface, head.row).length
      const crosses =
        (event.key === 'ArrowLeft' && head.offset === 0) ||
        (event.key === 'ArrowRight' && head.offset === length) ||
        (event.key === 'ArrowUp' && head.row === 0 && head.block > 0) ||
        (event.key === 'ArrowDown' && head.row === rowCount(surface) - 1 && head.block < base.blocks.length - 1)
      if (!crosses) return
      event.preventDefault()
      const moved =
        event.key === 'ArrowLeft'
          ? moveLeft(base, event.shiftKey)
          : event.key === 'ArrowRight'
            ? moveRight(base, event.shiftKey)
            : event.key === 'ArrowUp'
              ? moveUp(base, event.shiftKey)
              : moveDown(base, event.shiftKey)
      if (moved === base) return
      apply(moved, false)
    }
  }

  function handlePaste(event: ReactClipboardEvent<HTMLDivElement>): void {
    const text = event.clipboardData.getData('text/plain')
    event.preventDefault()
    if (text === '') return
    const target = host.current
    if (target === null) return
    const selection = readSelection(target)
    const base = selection === null ? stateRef.current : withSelection(stateRef.current, selection.anchor, selection.head)
    apply(paste(base, text), false)
  }

  function handleCopy(event: ReactClipboardEvent<HTMLDivElement>, cut: boolean): void {
    const target = host.current
    if (target === null) return
    const selection = readSelection(target)
    if (selection === null) return
    const base = withSelection(stateRef.current, selection.anchor, selection.head)
    const text = selectedText(base)
    if (text === '') return
    event.preventDefault()
    event.clipboardData.setData('text/plain', text)
    if (cut) apply(deleteSelection(base), false)
  }

  function updateLanguage(index: number, language: string): void {
    const surface = stateRef.current.blocks[index]
    if (surface === undefined || surface.kind !== 'code' || surface.language === language) return
    const next: EditorState = {
      ...stateRef.current,
      blocks: stateRef.current.blocks.map((block, i) =>
        i === index ? { ...surface, language, edited: true } : block,
      ),
    }
    // A language change is an input-originated edit: it must not claim the
    // note's caret, or the layout effect would move focus out of the field
    // after one character.
    historyRef.current = record(historyRef.current, stateRef.current, { merge: true })
    stateRef.current = next
    lastText.current = serialiseDocument(toDocument(next))
    setState(next)
    onChange(toDocument(next))
  }

  function checkTask(block: number, row: number): void {
    apply(toggleTask(stateRef.current, block, row), false)
  }

  return (
    <div
      ref={host}
      className="blocks"
      role="textbox"
      aria-label="Note text"
      aria-multiline="true"
      contentEditable
      suppressContentEditableWarning
      spellCheck={false}
      style={{ fontSize: `${fontSize}px` }}
      onInput={(event) => {
        if ((event.target as HTMLElement).closest('input, textarea, select, button') !== null) return
        handleInput()
      }}
      onKeyDown={handleKeyDown}
      onKeyUp={() => reportCaret()}
      onMouseUp={() => reportCaret()}
      onPaste={handlePaste}
      onCopy={(event) => handleCopy(event, false)}
      onCut={(event) => handleCopy(event, true)}
      onCompositionStart={() => {
        composing.current = true
      }}
      onCompositionEnd={() => {
        composing.current = false
        handleInput()
      }}
    >
      {state.blocks.map((surface, index) => (
        <BlockView
          key={index}
          index={index}
          surface={surface}
          notePath={notePath}
          selection={state.selection}
          noteIndex={noteIndex}
          onOpenNote={onOpenNote}
          onCheck={(row) => checkTask(index, row)}
          onLanguage={(language) => updateLanguage(index, language)}
          onSwitchToSource={onSwitchToSource}
        />
      ))}
      {picker === null ? null : (
        <div
          className="wikilink-picker-anchor"
          contentEditable={false}
          style={{ left: `${picker.left}px`, top: `${picker.top}px` }}
        >
          <WikilinkPicker
            notes={noteIndex}
            query={picker.query}
            active={picker.active}
            onChoose={choose}
          />
        </div>
      )}
    </div>
  )
}

function textOf(surface: SurfaceBlock, row: number): string {
  return rowText(surface, row)
}

type BlockViewProps = {
  index: number
  surface: SurfaceBlock
  notePath: string
  selection: Selection
  noteIndex: string[]
  onOpenNote?: (path: string) => void
  onCheck: (row: number) => void
  onLanguage: (language: string) => void
  onSwitchToSource: (block: number) => void
}

function BlockView({
  index,
  surface,
  notePath,
  selection,
  noteIndex,
  onOpenNote,
  onCheck,
  onLanguage,
  onSwitchToSource,
}: BlockViewProps) {
  const [imageFailed, setImageFailed] = useState(false)
  const common = {
    className: `block block--${surface.kind}`,
    'data-block': index,
    'data-kind': surface.kind,
  }
  switch (surface.kind) {
    case 'paragraph':
    case 'heading':
      return (
        <div
          {...common}
          className={`${common.className}${surface.kind === 'heading' ? ' block--h' : ''}`}
          data-level={surface.kind === 'heading' ? surface.level ?? 1 : undefined}
        >
          <span className="block__text" data-text data-row="0">
            <InlineRuns
              source={textOf(surface, 0)}
              notes={noteIndex}
              selection={revealFor(selection, index, 0)}
              onOpenNote={onOpenNote}
            />
          </span>
        </div>
      )
    case 'list':
    case 'task':
    case 'quote': {
      const task = surface.kind === 'task'
      const quote = surface.kind === 'quote'
      const listClass = quote
        ? 'block--quote'
        : surface.kind === 'task'
          ? 'block--task'
          : surface.ordered === true
            ? 'block--ordered'
            : 'block--bullet'
      return (
        <div {...common} className={`block block--${surface.kind} ${listClass}`}>
          {surface.rows.map((row, rowIndex) => (
            <div className="block__row" data-row={rowIndex} key={rowIndex}>
              {task ? (
                <button
                  type="button"
                  role="checkbox"
                  aria-checked={row.checked === true}
                  aria-label="Toggle task"
                  className="block__check"
                  contentEditable={false}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => onCheck(rowIndex)}
                >
                  {row.checked === true ? '\u2713' : ''}
                </button>
              ) : (
                <span className="block__marker" contentEditable={false} aria-hidden="true">
                  {marker(surface, rowIndex)}
                </span>
              )}
              <span className="block__text" data-text>
                <InlineRuns
                  source={row.text}
                  notes={noteIndex}
                  selection={revealFor(selection, index, rowIndex)}
                  onOpenNote={onOpenNote}
                />
              </span>
            </div>
          ))}
        </div>
      )
    }
    case 'code':
      return (
        <div {...common} className="block block--code">
          <div className="block__code-head" contentEditable={false}>
            <input
              className="block__code-lang"
              aria-label="Code language"
              value={surface.language ?? ''}
              placeholder="language"
              onChange={(event) => onLanguage(event.target.value)}
              onKeyDown={(event) => event.stopPropagation()}
              onPaste={(event) => event.stopPropagation()}
              onCopy={(event) => event.stopPropagation()}
              onCut={(event) => event.stopPropagation()}
            />
          </div>
          <pre className="block__code-body">
            <span className="block__text" data-text data-row="0">
              {surface.text}
            </span>
          </pre>
        </div>
      )
    case 'math':
      return (
        <div {...common} className="block block--math">
          <pre className="block__math-source">
            <span className="block__math-line" contentEditable={false} aria-hidden="true">
              {'$$'}
            </span>
            <span className="block__text" data-text data-row="0">
              {surface.text}
            </span>
            <span className="block__math-line" contentEditable={false} aria-hidden="true">
              {'$$'}
            </span>
          </pre>
        </div>
      )
    case 'rule':
      return (
        <div {...common} className="block block--rule" contentEditable={false}>
          <hr className="block__rule" />
        </div>
      )
    case 'image': {
      const target = resolveImageTarget(surface.destination, notePath)
      const src =
        target.kind === 'bikeshed' && target.path !== '' && !imageFailed
          ? assetUrl(target.path)
          : null
      return (
        <figure {...common} className="block block--image" contentEditable={false}>
          {src === null ? (
            <div className="block__image-placeholder">
              <span className="block__image-alt">{surface.alt}</span>
              <span className="block__image-target">{surface.destination}</span>
            </div>
          ) : (
            <img
              className="block__image-img"
              src={src}
              alt={surface.alt}
              onError={() => setImageFailed(true)}
            />
          )}
        </figure>
      )
    }
    case 'opaque':
      return (
        <div {...common} className="block block--opaque" contentEditable={false}>
          <pre className="block__opaque-source">{surface.source}</pre>
          <button
            type="button"
            className="block__source-button"
            onClick={() => onSwitchToSource(index)}
          >
            Edit in Source mode
          </button>
        </div>
      )
  }
}

function marker(surface: SurfaceBlock, row: number): string {
  if (surface.kind === 'task') return surface.rows[row]?.checked === true ? '- [x] ' : '- [ ] '
  if (surface.kind === 'quote') return '> '
  if (surface.kind === 'list' && surface.ordered === true) return `${(surface.start ?? 1) + row}. `
  if (surface.kind === 'list') return '- '
  return ''
}

function same(a: Position, b: Position): boolean {
  return a.block === b.block && a.row === b.row && a.offset === b.offset
}

function revealFor(
  selection: Selection,
  block: number,
  row: number,
): { from: number; to: number } | null {
  const { anchor, head } = selection
  if (anchor.block !== block || head.block !== block) return null
  if (anchor.row !== row || head.row !== row) return null
  return {
    from: Math.min(anchor.offset, head.offset),
    to: Math.max(anchor.offset, head.offset),
  }
}

function revealKey(selection: Selection): string {
  const { anchor, head } = selection
  return `${anchor.block}:${anchor.row}:${anchor.offset}:${head.block}:${head.row}:${head.offset}`
}

/** Whether the caret's row is plain text, so a keystroke can skip a render. */
function plainRow(state: EditorState): boolean {
  const head = state.selection.head
  const surface = state.blocks[head.block]
  if (surface === undefined || !isTextSurface(surface)) return true
  return parseInline(rowText(surface, head.row)).every(
    (run) => run.kind === 'text' && run.styles.length === 0,
  )
}

function isTextSurface(surface: SurfaceBlock): surface is Extract<SurfaceBlock, { rows: unknown }> {
  return (
    surface.kind === 'paragraph' ||
    surface.kind === 'heading' ||
    surface.kind === 'list' ||
    surface.kind === 'quote' ||
    surface.kind === 'task'
  )
}

/** Bring every text row back to the live model after a structural render. */
function syncDomText(host: HTMLElement, state: EditorState): void {
  const elements = Array.from(host.querySelectorAll('[data-block]')) as HTMLElement[]
  for (const element of elements) {
    const index = Number(element.dataset['block'])
    const surface = state.blocks[index]
    if (surface === undefined) continue
    const texts = Array.from(element.querySelectorAll('[data-text]')) as HTMLElement[]
    if (surface.kind === 'code' || surface.kind === 'math') {
      const text = texts[0]
      if (text !== undefined && text.textContent !== surface.text) text.textContent = surface.text
      continue
    }
    if (!isTextSurface(surface)) continue
    surface.rows.forEach((row, rowIndex) => {
      const text = texts[rowIndex]
      if (text !== undefined && text.textContent !== row.text) text.textContent = row.text
    })
  }
}

/** Read the edited text back from the DOM, leaving untouched blocks alone. */
function readFromDom(host: HTMLElement, current: EditorState): EditorState {
  const blocks = current.blocks.map((block) => block)
  const elements = Array.from(host.querySelectorAll('[data-block]')) as HTMLElement[]
  for (const element of elements) {
    const index = Number(element.dataset['block'])
    const surface = blocks[index]
    if (surface === undefined) continue
    const texts = Array.from(element.querySelectorAll('[data-text]')) as HTMLElement[]
    if (surface.kind === 'code' || surface.kind === 'math') {
      const text = texts[0]?.textContent ?? surface.text
      if (text !== surface.text) blocks[index] = { ...surface, text, edited: true }
      continue
    }
    if (!isTextSurface(surface)) continue
    const rows = surface.rows.map((row, rowIndex) => ({
      ...row,
      text: texts[rowIndex]?.textContent ?? row.text,
    }))
    const changed = rows.some((row, rowIndex) => row.text !== surface.rows[rowIndex]?.text)
    if (changed) blocks[index] = { ...surface, rows, edited: true }
  }
  return { ...current, blocks }
}

function positionFrom(host: HTMLElement, node: Node | null, offset: number): Position | null {
  if (node === null) return null
  const element = node.nodeType === Node.TEXT_NODE ? node.parentElement : (node as HTMLElement)
  const blockElement = element?.closest('[data-block]') as HTMLElement | null
  if (blockElement === null || !host.contains(blockElement)) return null
  const block = Number(blockElement.dataset['block'])
  const rowElement = (element?.closest('[data-row]') as HTMLElement | null) ?? blockElement
  const row = Number(rowElement.dataset['row'] ?? '0')
  const textElement = rowElement.matches('[data-text]')
    ? rowElement
    : (rowElement.querySelector('[data-text]') as HTMLElement | null)
  if (textElement === null) return { block, row, offset: 0 }
  if (!textElement.contains(node)) {
    const length = textElement.textContent?.length ?? 0
    if (node === textElement.parentNode) {
      const index = Array.prototype.indexOf.call(node.childNodes, textElement)
      return { block, row, offset: offset <= index ? 0 : length }
    }
    return { block, row, offset: node === textElement ? 0 : length }
  }
  const range = document.createRange()
  range.selectNodeContents(textElement)
  range.setEnd(node, offset)
  return { block, row, offset: range.toString().length }
}

function readSelection(host: HTMLElement): Selection | null {
  if (typeof window === 'undefined') return null
  const selection = window.getSelection()
  if (selection === null || selection.rangeCount === 0) return null
  const anchor = positionFrom(host, selection.anchorNode, selection.anchorOffset)
  const head = positionFrom(host, selection.focusNode, selection.focusOffset)
  if (anchor === null || head === null) return null
  return { anchor, head }
}

function nodeAt(element: HTMLElement, offset: number): { node: Node; offset: number } {
  let remaining = offset
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT)
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    const length = node.nodeValue?.length ?? 0
    if (remaining <= length) return { node, offset: remaining }
    remaining -= length
  }
  return { node: element, offset: element.childNodes.length }
}

function pointAt(host: HTMLElement, position: Position): { node: Node; offset: number } | null {
  const block = host.querySelector(`[data-block="${position.block}"]`) as HTMLElement | null
  if (block === null) return null
  const row = (block.querySelector(`[data-row="${position.row}"]`) as HTMLElement | null) ?? block
  const text = row.matches('[data-text]')
    ? row
    : (row.querySelector('[data-text]') as HTMLElement | null)
  if (text === null) return { node: block, offset: 0 }
  return nodeAt(text, position.offset)
}

function setDomSelection(host: HTMLElement, selection: Selection): void {
  if (typeof window === 'undefined') return
  host.focus()
  const start = pointAt(host, selection.anchor)
  const end = pointAt(host, selection.head)
  if (start === null || end === null) return
  const range = document.createRange()
  try {
    range.setStart(start.node, start.offset)
    range.setEnd(end.node, end.offset)
  } catch {
    return
  }
  const dom = window.getSelection()
  if (dom === null) return
  dom.removeAllRanges()
  dom.addRange(range)
}
