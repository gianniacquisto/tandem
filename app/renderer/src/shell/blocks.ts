import {
  parser,
  Strikethrough,
  Table,
  TaskList,
  type BlockParser,
  type MarkdownConfig,
  type MarkdownExtension,
} from '@lezer/markdown'

/**
 * The bikeshed editor's block model over a note's markdown.
 *
 * The `.md` file is the source of truth and the model is a view of it, so the
 * document is a total partition of the text: an ordered sequence of gaps and
 * blocks whose concatenation is the source exactly. A block carries the parser
 * node's exact source bytes; whatever no node claims becomes an opaque block or
 * a gap. `serialiseDocument(parseDocument(text)) === text` for any string, and
 * the only thing that ever re-renders is a block the editor marked edited. That
 * rule is what lets a table, frontmatter, raw HTML or an unrecognised construct
 * survive a save without a per-construct case.
 *
 * Inline content is one raw source string per block in this ticket. Bold,
 * italic, links, wikilinks and inline math become runs in UI-19.
 *
 * No React and no `window`: the model is pure, so it is tested in Node.
 */

export type Document = { parts: Part[] }

export type Part = GapPart | BlockPart

export type GapPart = { type: 'gap'; source: string }

export type BlockPart = { type: 'block'; block: Block }

export type BlockBase = {
  /** The parser node's exact source bytes. */
  source: string
  /** Only an edited block renders from its model; an untouched one emits `source`. */
  edited: boolean
}

export type ParagraphBlock = BlockBase & { kind: 'paragraph'; text: string }

export type HeadingBlock = BlockBase & {
  kind: 'heading'
  level: number
  text: string
}

export type ListBlock = BlockBase & {
  kind: 'list'
  ordered: boolean
  /** The first item's authored number for an ordered list; 1 for a bullet list. */
  start: number
  /** Each item is its own block sequence, so a nested list is one of its blocks. */
  items: Block[][]
}

export type TaskBlock = BlockBase & {
  kind: 'task'
  checked: boolean
  blocks: Block[]
}

export type QuoteBlock = BlockBase & { kind: 'quote'; blocks: Block[] }

export type CodeBlock = BlockBase & {
  kind: 'code'
  language: string | null
  text: string
}

export type RuleBlock = BlockBase & { kind: 'rule' }

export type ImageBlock = BlockBase & {
  kind: 'image'
  alt: string
  destination: string
  title: string | null
}

/**
 * A construct the model does not cover, held as its exact source: frontmatter,
 * a table, raw HTML, display math, a footnote definition, or any range the
 * parser does not claim. One kind, not one per construct.
 */
export type OpaqueBlock = BlockBase & { kind: 'opaque' }

export type Block =
  | ParagraphBlock
  | HeadingBlock
  | ListBlock
  | TaskBlock
  | QuoteBlock
  | CodeBlock
  | RuleBlock
  | ImageBlock
  | OpaqueBlock

/** The parser's node, as much of it as the model reads. */
type TreeNode = {
  readonly name: string
  readonly from: number
  readonly to: number
  readonly firstChild: TreeNode | null
  readonly nextSibling: TreeNode | null
  readonly type: { is(...names: string[]): boolean }
}

type Range = { from: number; to: number }

// A line whose content is exactly `$$` opens a display-math block; the closing
// line is another such line. The single-line form mirrors the editor bundle's
// scanner (`Editor/src/pure/math.js`).
const DISPLAY_LINE = '$$'
const SINGLE_LINE_DISPLAY = /^ {0,3}\$\$([^\s$](?:[^$]*[^\s$])?)\$\$\s*$/

/** The editor bundle's strict frontmatter rule (`Editor/src/pure/frontmatter.js`). */
function isFrontmatterDelimiter(line: string): boolean {
  return line === '---' || line === '---\r'
}

/**
 * The end of a leading, properly terminated frontmatter block: the offset just
 * past the closing delimiter line's content, not its line terminator. `null`
 * when the document has no such block, so an unterminated fence stays text.
 */
function frontmatterEnd(text: string): number | null {
  const firstBreak = text.indexOf('\n')
  if (firstBreak === -1) return null
  if (!isFrontmatterDelimiter(text.slice(0, firstBreak))) return null

  let pos = firstBreak + 1
  for (;;) {
    const nextBreak = text.indexOf('\n', pos)
    const lineEnd = nextBreak === -1 ? text.length : nextBreak
    if (isFrontmatterDelimiter(text.slice(pos, lineEnd))) return lineEnd
    if (nextBreak === -1) return null
    pos = nextBreak + 1
  }
}

type SourceLine = { start: number; end: number; content: string }

function sourceLines(text: string, from: number): SourceLine[] {
  const lines: SourceLine[] = []
  let pos = from
  while (pos < text.length) {
    const nextBreak = text.indexOf('\n', pos)
    const end = nextBreak === -1 ? text.length : nextBreak
    lines.push({ start: pos, end, content: text.slice(pos, end) })
    if (nextBreak === -1) break
    pos = nextBreak + 1
  }
  return lines
}

/**
 * The display-math ranges of a note, recognised before the markdown tree can
 * split them into paragraphs. Lines inside a fence are harmless: their starts
 * are never the start of a block, so the parser never claims them.
 */
function displayMathRanges(text: string, from: number): Range[] {
  const lines = sourceLines(text, from)
  const ranges: Range[] = []
  const consumed = new Set<number>()

  for (let i = 0; i < lines.length; i++) {
    if (consumed.has(i)) continue
    const opener = lines[i]!
    const indent = opener.content.length - opener.content.trimStart().length
    if (indent > 3) continue

    if (opener.content.trim() === DISPLAY_LINE) {
      for (let j = i + 1; j < lines.length; j++) {
        if (lines[j]!.content.trim() !== DISPLAY_LINE) continue
        ranges.push({ from: opener.start, to: lines[j]!.end })
        for (let k = i; k <= j; k++) consumed.add(k)
        break
      }
      continue
    }

    if (SINGLE_LINE_DISPLAY.test(opener.content)) {
      ranges.push({ from: opener.start, to: opener.end })
      consumed.add(i)
    }
  }

  return ranges
}

function frontmatterParser(end: number): BlockParser {
  return {
    name: 'Frontmatter',
    before: 'HorizontalRule',
    parse(cx, line) {
      if (cx.lineStart !== 0) return false
      while (cx.lineStart + line.text.length < end) {
        if (!cx.nextLine()) return false
      }
      cx.addElement(cx.elt('Frontmatter', 0, end))
      cx.nextLine()
      return true
    },
  }
}

function displayMathParser(ranges: Range[]): BlockParser {
  return {
    name: 'DisplayMath',
    before: 'IndentedCode',
    parse(cx, line) {
      const range = ranges.find((candidate) => candidate.from === cx.lineStart)
      if (range === undefined) return false
      while (cx.lineStart + line.text.length < range.to) {
        if (!cx.nextLine()) break
      }
      cx.addElement(cx.elt('DisplayMath', range.from, range.to))
      cx.nextLine()
      return true
    },
  }
}

/** A `[^x]:` line at the start of a block, recognised before it becomes a paragraph. */
const footnoteParser: BlockParser = {
  name: 'Footnote',
  parse(cx, line) {
    const content = line.text.slice(line.pos)
    if (!/^\[\^[^\]]+\]:(\s|$)/.test(content)) return false
    const from = cx.lineStart + line.pos
    const to = cx.lineStart + line.text.length
    cx.addElement(cx.elt('Footnote', from, to))
    cx.nextLine()
    return true
  },
}

const dialect: MarkdownConfig[] = [Table, TaskList, Strikethrough]
const nodeSpecs = [
  { name: 'Frontmatter', block: true },
  { name: 'DisplayMath', block: true },
  { name: 'Footnote', block: true },
]

function markdownParser(text: string) {
  const frontmatter = frontmatterEnd(text)
  const math = displayMathRanges(text, frontmatter ?? 0)
  const parseBlock: BlockParser[] = []
  if (frontmatter !== null) parseBlock.push(frontmatterParser(frontmatter))
  if (math.length > 0) parseBlock.push(displayMathParser(math))
  parseBlock.push(footnoteParser)
  const custom: MarkdownExtension = { defineNodes: nodeSpecs, parseBlock }
  return parser.configure([...dialect, custom])
}

function children(node: TreeNode): TreeNode[] {
  const out: TreeNode[] = []
  for (let child = node.firstChild; child !== null; child = child.nextSibling) out.push(child)
  return out
}

/** A composite's own blocks: markers such as `>` and `-` are not blocks. */
function blockChildren(node: TreeNode): TreeNode[] {
  return children(node).filter((child) => child.type.is('Block'))
}

function inlineText(source: string): string {
  return source.replace(/\r$/, '')
}

/**
 * A paragraph's inline content: its exact source with any container markers the
 * parser nested inside it removed. In a multi-line blockquote paragraph the
 * `>` markers are children of the Paragraph node, so their bytes are container
 * syntax, not text.
 */
function inlineSource(node: TreeNode, text: string): string {
  const markers = children(node).filter(
    (child) => child.name === 'QuoteMark' || child.name === 'ListMark',
  )
  if (markers.length === 0) return inlineText(text.slice(node.from, node.to))

  let out = ''
  let at = node.from
  for (const marker of markers) {
    out += text.slice(at, marker.from)
    at = marker.to
    if (text[at] === ' ') at++
  }
  out += text.slice(at, node.to)
  return inlineText(out)
}

function paragraphBlock(node: TreeNode, text: string): Block {
  const source = text.slice(node.from, node.to)
  const kids = children(node)
  const image = kids.length === 1 ? kids[0] : undefined
  if (
    image !== undefined &&
    image.name === 'Image' &&
    image.from === node.from &&
    image.to === node.to
  ) {
    const decoded = decodeImage(source)
    if (decoded !== null) return { kind: 'image', ...decoded, source, edited: false }
  }
  return { kind: 'paragraph', text: inlineSource(node, text), source, edited: false }
}

function headingBlock(node: TreeNode, text: string, level: number): HeadingBlock {
  const source = text.slice(node.from, node.to)
  let body: string
  if (node.name.startsWith('ATX')) {
    body = source.replace(/^#{1,6}[ \t]*/, '').replace(/[ \t]+#+[ \t]*$/, '')
  } else {
    const firstBreak = source.indexOf('\n')
    body = firstBreak === -1 ? source : source.slice(0, firstBreak)
  }
  return { kind: 'heading', level, text: body.replace(/\r$/, '').trim(), source, edited: false }
}

function listBlock(node: TreeNode, text: string): ListBlock {
  const ordered = node.name === 'OrderedList'
  const items = children(node)
    .filter((child) => child.name === 'ListItem')
    .map((item) => listItemBlocks(item, text))
  return {
    kind: 'list',
    ordered,
    start: ordered ? orderedStart(node, text) : 1,
    items,
    source: text.slice(node.from, node.to),
    edited: false,
  }
}

function orderedStart(node: TreeNode, text: string): number {
  const firstItem = children(node).find((child) => child.name === 'ListItem')
  if (firstItem === undefined) return 1
  const mark = children(firstItem).find((child) => child.name === 'ListMark')
  if (mark === undefined) return 1
  const value = Number.parseInt(text.slice(mark.from, mark.to), 10)
  return Number.isNaN(value) ? 1 : value
}

function listItemBlocks(item: TreeNode, text: string): Block[] {
  const kids = blockChildren(item)
  const first = kids[0]
  if (first !== undefined && first.name === 'Task') {
    const task = taskBlock(first, text)
    task.blocks.push(...kids.slice(1).map((child) => toBlock(child, text)))
    return [task]
  }
  return kids.map((child) => toBlock(child, text))
}

function taskBlock(node: TreeNode, text: string): TaskBlock {
  const source = text.slice(node.from, node.to)
  const checked = source[1] === 'x' || source[1] === 'X'
  const body = source.slice(/^\[[ xX]\][ \t]/.test(source) ? 4 : 3)
  const blocks: Block[] =
    body === ''
      ? []
      : [{ kind: 'paragraph', text: inlineText(body), source: body, edited: false }]
  return { kind: 'task', checked, blocks, source, edited: false }
}

function quoteBlock(node: TreeNode, text: string): QuoteBlock {
  return {
    kind: 'quote',
    blocks: blockChildren(node).map((child) => toBlock(child, text)),
    source: text.slice(node.from, node.to),
    edited: false,
  }
}

function codeBlock(node: TreeNode, text: string, from: number): CodeBlock {
  let language: string | null = null
  const bodies: string[] = []
  for (const child of children(node)) {
    if (child.name === 'CodeInfo') {
      language = text.slice(child.from, child.to).trim() || null
    } else if (child.name === 'CodeText') {
      bodies.push(text.slice(child.from, child.to))
    }
  }
  return {
    kind: 'code',
    language,
    text: bodies.join(''),
    source: text.slice(from, node.to),
    edited: false,
  }
}

function toBlock(node: TreeNode, text: string, from = node.from): Block {
  const source = text.slice(from, node.to)
  if (node.name === 'Paragraph') return paragraphBlock(node, text)
  if (node.name.startsWith('ATXHeading')) {
    return headingBlock(node, text, Number.parseInt(node.name.slice(10), 10))
  }
  if (node.name === 'SetextHeading1') return headingBlock(node, text, 1)
  if (node.name === 'SetextHeading2') return headingBlock(node, text, 2)
  if (node.name === 'BulletList' || node.name === 'OrderedList') return listBlock(node, text)
  if (node.name === 'Task') return taskBlock(node, text)
  if (node.name === 'Blockquote') return quoteBlock(node, text)
  if (node.name === 'FencedCode' || node.name === 'CodeBlock') return codeBlock(node, text, from)
  if (node.name === 'HorizontalRule') return { kind: 'rule', source, edited: false }
  // Frontmatter, a table, raw HTML, display math, a footnote definition and
  // anything else the model does not cover: one opaque block with its bytes.
  return { kind: 'opaque', source, edited: false }
}

/** One top-level image written as `![alt](destination "title")`, or null. */
function decodeImage(
  source: string,
): { alt: string; destination: string; title: string | null } | null {
  const match = /^!\[([^\]]*)\]\(\s*(<[^>]*>|\S+?)(?:\s+(?:"([^"]*)"|'([^']*)'))?\s*\)$/.exec(
    source,
  )
  if (match === null) return null
  const destination = match[2]
  if (destination === undefined) return null
  const title = match[3] ?? match[4] ?? null
  return { alt: match[1] ?? '', destination, title }
}

/**
 * The first byte a block owns. An indented code node starts after its four-space
 * indent, but that indent is the construct's marker, not a gap between blocks,
 * so the block itself holds it; an edit then writes a fence, not an indented one.
 */
function blockStart(node: TreeNode, text: string): number {
  if (node.name !== 'CodeBlock') return node.from
  return text.lastIndexOf('\n', node.from - 1) + 1
}

/** Parse a note into the byte-exact partition the editor edits. */
export function parseDocument(text: string): Document {
  const tree = markdownParser(text).parse(text)
  const parts: Part[] = []
  let cursor = 0

  const gap = (to: number): void => {
    if (to <= cursor) return
    parts.push({ type: 'gap', source: text.slice(cursor, to) })
    cursor = to
  }

  for (let node = tree.topNode.firstChild; node !== null; node = node.nextSibling) {
    const from = blockStart(node, text)
    gap(from)
    parts.push({ type: 'block', block: toBlock(node, text, from) })
    cursor = node.to
  }
  gap(text.length)

  return { parts }
}

/** The document's source: untouched blocks verbatim, edited blocks canonically. */
export function serialiseDocument(document: Document): string {
  let out = ''
  for (const part of document.parts) {
    if (part.type === 'gap') out += part.source
    else out += part.block.edited ? renderBlock(part.block) : part.block.source
  }
  return out
}

/** Replace the block at `index` and mark it edited, so it renders from its model. */
export function editBlock(document: Document, index: number, block: Block): Document {
  let seen = 0
  const parts = document.parts.map((part): Part => {
    if (part.type !== 'block') return part
    if (seen++ !== index) return part
    return { type: 'block', block: { ...block, edited: true } }
  })
  if (seen <= index) throw new RangeError(`no block at index ${index}`)
  return { parts }
}

/** The document's blocks, in order, without the gaps. */
export function documentBlocks(document: Document): Block[] {
  const blocks: Block[] = []
  for (const part of document.parts) {
    if (part.type === 'block') blocks.push(part.block)
  }
  return blocks
}

function renderBlocks(blocks: Block[]): string {
  return blocks.map(renderBlock).join('\n')
}

function indentTail(text: string, width: number): string {
  const pad = ' '.repeat(width)
  return text
    .split('\n')
    .map((line, index) => (index === 0 ? line : pad + line))
    .join('\n')
}

/** Render a block from its model, in the one canonical spelling for its kind. */
function renderBlock(block: Block): string {
  switch (block.kind) {
    case 'paragraph':
      return block.text
    case 'heading':
      return `${'#'.repeat(block.level)} ${block.text}`
    case 'list':
      return block.items.map((item, index) => renderItem(item, block, index)).join('\n')
    case 'task':
      return renderTask(block)
    case 'quote':
      return renderBlocks(block.blocks)
        .split('\n')
        .map((line) => (line === '' ? '>' : `> ${line}`))
        .join('\n')
    case 'code':
      return renderCode(block)
    case 'rule':
      return '---'
    case 'image': {
      const title = block.title === null ? '' : ` "${block.title.replace(/"/g, '\\"')}"`
      return `![${block.alt}](${block.destination}${title})`
    }
    case 'opaque':
      throw new Error('an opaque block has no canonical rendering')
  }
}

function renderItem(item: Block[], list: ListBlock, index: number): string {
  const only = item[0]
  if (item.length === 1 && only !== undefined && only.kind === 'task') return renderTask(only)
  const marker = list.ordered ? `${list.start + index}. ` : '- '
  const body = renderBlocks(item)
  return body === '' ? marker.trimEnd() : marker + indentTail(body, marker.length)
}

function renderTask(block: TaskBlock): string {
  const marker = block.checked ? '- [x] ' : '- [ ] '
  const body = renderBlocks(block.blocks)
  return body === '' ? marker.trimEnd() : marker + indentTail(body, marker.length)
}

function renderCode(block: CodeBlock): string {
  const info = block.language ?? ''
  return block.text === '' ? `\`\`\`${info}\n\`\`\`` : `\`\`\`${info}\n${block.text}\n\`\`\``
}
