import { Fragment, type MouseEvent, type ReactNode } from 'react'

import { resolveWikilink } from '../shell/bikeshed'
import {
  filterNotes,
  parseInline,
  type InlineDelimiter,
  type InlineRun,
  type InlineStyle,
  type NoteChoice,
} from '../shell/inline'

/**
 * The drawing half of the inline layer: runs with their markers hidden, the
 * markers revealed inside the run that holds the caret, a wikilink chip, a
 * link run and math as its source text.
 *
 * The markers are real text nodes hidden with CSS, not absent from the DOM:
 * the Write surface reads its source back from the row's `textContent`, so a
 * hidden marker is what keeps the caret's offsets and the block's bytes the
 * same string. The chip's display text is the visible part and its `[[…|`
 * prefix is the hidden part.
 */

const STYLE_ORDER: InlineStyle[] = ['bold', 'italic', 'strikethrough', 'code']

const DELIMITER: Record<InlineStyle, string> = {
  bold: '**',
  italic: '*',
  strikethrough: '~~',
  code: '`',
}

export type InlineSelection = { from: number; to: number }

export type InlineRunsProps = {
  source: string
  notes?: string[]
  selection?: InlineSelection | null
  onOpenNote?: (path: string) => void
}

function touches(run: InlineRun, selection: InlineSelection | null | undefined): boolean {
  if (selection === null || selection === undefined) return false
  if (selection.from === selection.to) {
    return selection.from >= run.from && selection.from <= run.to
  }
  return selection.from < run.to && selection.to > run.from
}

function wrap(style: InlineStyle, node: ReactNode): ReactNode {
  switch (style) {
    case 'bold':
      return <strong>{node}</strong>
    case 'italic':
      return <em>{node}</em>
    case 'strikethrough':
      return <del>{node}</del>
    case 'code':
      return <code>{node}</code>
  }
}

function styled(styles: InlineStyle[], node: ReactNode): ReactNode {
  let out = node
  for (const style of [...STYLE_ORDER].reverse()) {
    if (styles.includes(style)) out = wrap(style, out)
  }
  return out
}

function marker(
  style: InlineStyle,
  side: 'open' | 'close',
  shown: boolean,
  key: string,
  delimiter = DELIMITER[style],
): ReactNode {
  return (
    <span
      key={key}
      className={shown ? 'run__marker run__marker--shown' : 'run__marker'}
      data-marker={delimiter}
      data-side={side}
      aria-hidden={!shown}
    >
      {delimiter}
    </span>
  )
}

function linkTarget(href: string, notes: string[]): string | null {
  if (/^[a-z][a-z0-9+.-]*:/i.test(href)) return null
  if (href.startsWith('//') || href.startsWith('#')) return null
  return resolveWikilink(href, notes)
}

function runNode(
  run: InlineRun,
  notes: string[],
  shown: boolean,
  onOpenNote: ((path: string) => void) | undefined,
): ReactNode {
  if (run.kind === 'text') return styled(run.styles, run.text)

  if (run.kind === 'link') {
    const path = linkTarget(run.href, notes)
    return (
      <span className="run run--link">
        <span
          className={shown ? 'run__marker run__marker--shown' : 'run__marker'}
          aria-hidden={!shown}
        >
          [
        </span>
        <a
          className="run__anchor"
          title={run.href}
          data-href={run.href}
          href={path === null ? undefined : `#${path}`}
          onClick={(event: MouseEvent<HTMLAnchorElement>) => {
            if (path === null) return
            event.preventDefault()
            onOpenNote?.(path)
          }}
        >
          {styled(run.styles, run.text)}
        </a>
        <span
          className={shown ? 'run__marker run__marker--shown' : 'run__marker'}
          aria-hidden={!shown}
        >
          {`](${run.href})`}
        </span>
      </span>
    )
  }

  if (run.kind === 'wikilink') {
    const at = run.source.indexOf(run.display, 2)
    const prefix = at === -1 ? '[[' : run.source.slice(0, at)
    const suffix = at === -1 ? ']]' : run.source.slice(at + run.display.length)
    const broken = run.broken
    return (
      <span
        className={`run run--wikilink${broken ? ' run--wikilink-broken' : ''}`}
        data-target={run.target}
        title={broken ? `Missing note: ${run.target}` : run.display}
        role="link"
        tabIndex={broken ? undefined : 0}
        onClick={() => {
          if (broken || run.path === null) return
          onOpenNote?.(run.path)
        }}
      >
        <span className={shown ? 'run__marker run__marker--shown' : 'run__marker'} aria-hidden={!shown}>
          {prefix}
        </span>
        <span className={`wikilink-chip${broken ? ' wikilink-chip--broken' : ''}`}>{run.display}</span>
        <span className={shown ? 'run__marker run__marker--shown' : 'run__marker'} aria-hidden={!shown}>
          {suffix}
        </span>
      </span>
    )
  }

  return (
    <span className="run run--math" data-math={run.display ? 'display' : 'inline'}>
      {run.source}
    </span>
  )
}

/** Draw one block's inline source, markers hidden except inside the caret's run. */
export function InlineRuns({ source, notes = [], selection = null, onOpenNote }: InlineRunsProps) {
  const runs = parseInline(source, notes)
  const nodes: ReactNode[] = []
  const open: InlineStyle[] = []
  const openMarkers = new Map<InlineStyle, InlineDelimiter>()
  let lastShown = false

  runs.forEach((run, index) => {
    const styles = run.kind === 'text' || run.kind === 'link' ? run.styles : []
    const markers = run.kind === 'text' || run.kind === 'link' ? run.markers : undefined
    const shown = touches(run, selection)

    for (let position = open.length - 1; position >= 0; position--) {
      const style = open[position]
      if (style === undefined || styles.includes(style)) continue
      nodes.push(
        marker(
          style,
          'close',
          shown || lastShown,
          `close-${index}-${style}`,
          openMarkers.get(style)?.close ?? DELIMITER[style],
        ),
      )
      openMarkers.delete(style)
      open.splice(position, 1)
    }
    for (const style of STYLE_ORDER) {
      if (!styles.includes(style) || open.includes(style)) continue
      const delimiters = markers?.[style] ?? {
        open: DELIMITER[style],
        close: DELIMITER[style],
      }
      nodes.push(marker(style, 'open', shown, `open-${index}-${style}`, delimiters.open))
      openMarkers.set(style, delimiters)
      open.push(style)
    }
    nodes.push(<Fragment key={`run-${index}`}>{runNode(run, notes, shown, onOpenNote)}</Fragment>)
    lastShown = shown
  })

  for (let position = open.length - 1; position >= 0; position--) {
    const style = open[position]
    if (style === undefined) continue
    nodes.push(
      marker(
        style,
        'close',
        lastShown,
        `close-end-${style}`,
        openMarkers.get(style)?.close ?? DELIMITER[style],
      ),
    )
  }

  return <>{nodes}</>
}

export type WikilinkPickerProps = {
  notes: string[]
  query: string
  active: number
  onChoose?: (choice: NoteChoice) => void
}

/**
 * The `[[` picker: existing notes only, never a creation path. The keyboard is
 * the primary path, so the rows are options and the active one is announced;
 * a failure to match is a two-word empty state and Enter does nothing.
 */
export function WikilinkPicker({ notes, query, active, onChoose }: WikilinkPickerProps) {
  const choices = filterNotes(notes, query)
  return (
    <div className="wikilink-picker" role="listbox" aria-label="Notes">
      {choices.length === 0 ? (
        <div className="wikilink-picker__empty">No notes</div>
      ) : (
        choices.map((choice, index) => (
          <div
            key={choice.path}
            role="option"
            aria-selected={index === active}
            className={
              index === active ? 'wikilink-picker__row wikilink-picker__row--active' : 'wikilink-picker__row'
            }
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => onChoose?.(choice)}
          >
            <span className="wikilink-picker__name">{choice.label}</span>
            {choice.insert === choice.label ? null : (
              <span className="wikilink-picker__path">{choice.insert}</span>
            )}
          </div>
        ))
      )}
    </div>
  )
}
