import type { ReactNode } from 'react'

import type { MarkdownBlockView } from '../wire/MarkdownBlockView'
import type { MarkdownInlineView } from '../wire/MarkdownInlineView'

export type MarkdownDocumentProps = {
  /** The blocks the core parsed: the app draws them and parses nothing. */
  blocks: MarkdownBlockView[]
}

/**
 * A ticket's markdown, drawn (`MarkdownDocumentView` in the Swift app).
 *
 * The parsing is the core's (`markdown.blocks`), so the sheet and the CLI read
 * one dialect: this file is the drawing only. Links are text with their
 * destination as a title, because the app opens no remote content and a ticket
 * body is not a browser; images are their alt text for the same reason.
 *
 * Every field is read defensively: the wire promises the shapes, but one
 * drifting reply must never unmount the sheet (TAN-312). A missing container
 * draws nothing, and a block kind the app does not know draws a fallback
 * paragraph, never nothing (a component that returns undefined unmounts the
 * React root on the client).
 */
export function MarkdownDocument({ blocks }: MarkdownDocumentProps) {
  if (!Array.isArray(blocks) || blocks.length === 0) return null
  return (
    <div className="markdown">
      {blocks.map((block, index) => (
        <Block key={index} block={block} />
      ))}
    </div>
  )
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object'
}

function blockList(value: unknown): MarkdownBlockView[] {
  return Array.isArray(value) ? value : []
}

function Block({ block }: { block: MarkdownBlockView }) {
  if (!isObject(block)) return null
  const kind = (block as { kind?: unknown }).kind
  switch (kind) {
    case 'heading': {
      const level = block as { level?: unknown; runs?: unknown }
      const at = typeof level.level === 'number' ? level.level : 1
      const Tag = `h${Math.min(6, Math.max(1, at))}` as 'h1'
      return (
        <Tag className="markdown__heading">
          <Runs runs={(level as { runs?: MarkdownInlineView[] }).runs} />
        </Tag>
      )
    }
    case 'paragraph':
      return (
        <p className="markdown__paragraph">
          <Runs runs={(block as { runs?: MarkdownInlineView[] }).runs} />
        </p>
      )
    case 'list': {
      const list = block as { ordered?: unknown; start?: unknown; items?: unknown }
      const Tag = list.ordered === true ? 'ol' : 'ul'
      const items = Array.isArray(list.items) ? list.items : []
      return (
        <Tag className="markdown__list" start={Tag === 'ol' && typeof list.start === 'number' ? list.start : undefined}>
          {items.map((item, index) => (
            <li key={index} className="markdown__item">
              {blockList(item).map((inner, at) => (
                <Block key={at} block={inner} />
              ))}
            </li>
          ))}
        </Tag>
      )
    }
    case 'code': {
      const code = block as { language?: unknown; text?: unknown }
      const language = typeof code.language === 'string' ? code.language : undefined
      const text = typeof code.text === 'string' ? code.text : ''
      return (
        <pre className="markdown__code" data-language={language ?? undefined}>
          <code>{text}</code>
        </pre>
      )
    }
    case 'quote':
      return (
        <blockquote className="markdown__quote">
          {blockList((block as { blocks?: unknown }).blocks).map((inner, index) => (
            <Block key={index} block={inner} />
          ))}
        </blockquote>
      )
    case 'table': {
      const table = block as { header?: unknown; rows?: unknown }
      const header = Array.isArray(table.header) ? table.header : []
      const rows = Array.isArray(table.rows) ? table.rows : []
      return (
        <table className="markdown__table">
          {header.length === 0 ? null : (
            <thead>
              <tr>
                {header.map((cell, index) => (
                  <th key={index}>{typeof cell === 'string' ? cell : ''}</th>
                ))}
              </tr>
            </thead>
          )}
          <tbody>
            {rows.map((row, index) => (
              <tr key={index}>
                {(Array.isArray(row) ? row : []).map((cell, at) => (
                  <td key={at}>{typeof cell === 'string' ? cell : ''}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )
    }
    case 'rule':
      return <hr className="markdown__rule" />
    case 'task': {
      const task = block as { checked?: unknown; blocks?: unknown }
      const checked = task.checked === true
      return (
        <div className="markdown__task" data-checked={checked || undefined}>
          <span className="markdown__checkbox" role="checkbox" aria-checked={checked} />
          <div className="markdown__task-body">
            {blockList(task.blocks).map((inner, index) => (
              <Block key={index} block={inner} />
            ))}
          </div>
        </div>
      )
    }
    default:
      return <p className="markdown__paragraph">A block the app cannot draw yet.</p>
  }
}

function Runs({ runs }: { runs: MarkdownInlineView[] | undefined }) {
  if (!Array.isArray(runs)) return null
  return (
    <>
      {runs.map((run, index) => (
        <Run key={index} run={run} />
      ))}
    </>
  )
}

function Run({ run }: { run: MarkdownInlineView }) {
  if (!isObject(run)) return null
  const styles = Array.isArray((run as { styles?: unknown }).styles)
    ? (run as { styles: string[] }).styles
    : []
  const text = typeof (run as { text?: unknown }).text === 'string'
    ? (run as { text: string }).text
    : ''
  const link = typeof (run as { link?: unknown }).link === 'string'
    ? (run as { link: string }).link
    : null
  // The styles are a set, not a choice: `**bold with `code`**` is one run
  // carrying both, so each marker wraps the one inside it rather than the
  // first match returning. The link's label sits innermost, wearing the
  // styles around it.
  let body: ReactNode =
    link === null ? (
      text
    ) : (
      // The destination is a fact, not a door: the sheet opens no page.
      <span className="markdown__link" title={link}>
        {text}
      </span>
    )
  if (styles.includes('strikethrough')) body = <s>{body}</s>
  if (styles.includes('italic')) body = <em>{body}</em>
  if (styles.includes('bold')) body = <strong>{body}</strong>
  if (styles.includes('code')) body = <code className="markdown__inline-code">{body}</code>
  return <>{body}</>
}
