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
 */
export function MarkdownDocument({ blocks }: MarkdownDocumentProps) {
  if (blocks.length === 0) return null
  return (
    <div className="markdown">
      {blocks.map((block, index) => (
        <Block key={index} block={block} />
      ))}
    </div>
  )
}

function Block({ block }: { block: MarkdownBlockView }) {
  switch (block.kind) {
    case 'heading': {
      const Tag = `h${Math.min(6, Math.max(1, block.level))}` as 'h1'
      return (
        <Tag className="markdown__heading">
          <Runs runs={block.runs} />
        </Tag>
      )
    }
    case 'paragraph':
      return (
        <p className="markdown__paragraph">
          <Runs runs={block.runs} />
        </p>
      )
    case 'list': {
      const Tag = block.ordered ? 'ol' : 'ul'
      return (
        <Tag className="markdown__list" start={block.ordered ? block.start : undefined}>
          {block.items.map((item, index) => (
            <li key={index} className="markdown__item">
              {item.map((inner, at) => (
                <Block key={at} block={inner} />
              ))}
            </li>
          ))}
        </Tag>
      )
    }
    case 'code':
      return (
        <pre className="markdown__code" data-language={block.language ?? undefined}>
          <code>{block.text}</code>
        </pre>
      )
    case 'quote':
      return (
        <blockquote className="markdown__quote">
          {block.blocks.map((inner, index) => (
            <Block key={index} block={inner} />
          ))}
        </blockquote>
      )
    case 'table':
      return (
        <table className="markdown__table">
          {block.header.length === 0 ? null : (
            <thead>
              <tr>
                {block.header.map((cell, index) => (
                  <th key={index}>{cell}</th>
                ))}
              </tr>
            </thead>
          )}
          <tbody>
            {block.rows.map((row, index) => (
              <tr key={index}>
                {row.map((cell, at) => (
                  <td key={at}>{cell}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )
    case 'rule':
      return <hr className="markdown__rule" />
    case 'task':
      return (
        <div className="markdown__task" data-checked={block.checked || undefined}>
          <span className="markdown__checkbox" role="checkbox" aria-checked={block.checked} />
          <div className="markdown__task-body">
            {block.blocks.map((inner, index) => (
              <Block key={index} block={inner} />
            ))}
          </div>
        </div>
      )
  }
}

function Runs({ runs }: { runs: MarkdownInlineView[] }) {
  return (
    <>
      {runs.map((run, index) => (
        <Run key={index} run={run} />
      ))}
    </>
  )
}

function Run({ run }: { run: MarkdownInlineView }) {
  const body =
    run.link === null ? (
      run.text
    ) : (
      // The destination is a fact, not a door: the sheet opens no page.
      <span className="markdown__link" title={run.link}>
        {run.text}
      </span>
    )
  if (run.styles.includes('code')) return <code className="markdown__inline-code">{body}</code>
  if (run.styles.includes('bold')) return <strong>{body}</strong>
  if (run.styles.includes('italic')) return <em>{body}</em>
  if (run.styles.includes('strikethrough')) return <s>{body}</s>
  return <>{body}</>
}
