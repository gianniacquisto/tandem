import { resolveWikilink } from './bikeshed'

/**
 * The inline runs of one block.
 *
 * A per-keystroke parse cannot be a core round trip: the core's comrak model
 * has no wikilink, no math and no exact-source retention, so this module ports
 * the editor bundle's pure scanners (`Editor/src/pure/{scan,grammar,math,code,
 * frontmatter}.js` and the fixed point in `index.js`) into the renderer. The
 * union is deliberately small: a run carries its styles as a set, a link its
 * href, a wikilink its resolution, and math its source between delimiters.
 * `serialiseInline(parseInline(source))` is a stable fixed point; only an edited
 * block is ever rendered from it, so canonical delimiters are allowed there.
 *
 * No React and no `window`: this is the pure half of UI-19, tested in Node.
 */

export type InlineStyle = 'bold' | 'italic' | 'strikethrough' | 'code'

export type InlineDelimiter = { open: string; close: string }

/** The delimiter each style wore in the source, so a run draws and serialises as written. */
export type InlineDelimiters = Partial<Record<InlineStyle, InlineDelimiter>>

export type Range = { from: number; to: number }

export type TextRun = Range & {
  kind: 'text'
  text: string
  /** The set of styles, in canonical order (`['bold', 'code']` for **`code`**). */
  styles: InlineStyle[]
  markers?: InlineDelimiters
}

export type LinkRun = Range & {
  kind: 'link'
  text: string
  href: string
  styles: InlineStyle[]
  markers?: InlineDelimiters
}

export type WikilinkRun = Range & {
  kind: 'wikilink'
  target: string
  fragment?: string
  alias?: string
  display: string
  /** The note the renderer's resolver names, or null when the coin is broken. */
  path: string | null
  broken: boolean
  /** The construct exactly as written, delimiters included. */
  source: string
}

export type MathRun = Range & {
  kind: 'math'
  display: boolean
  content: string
  /** The construct exactly as written, delimiters included. */
  source: string
}

export type InlineRun = TextRun | LinkRun | WikilinkRun | MathRun

const STYLE_ORDER: InlineStyle[] = ['bold', 'italic', 'strikethrough', 'code']

const DELIMITER: Record<InlineStyle, string> = {
  bold: '**',
  italic: '*',
  strikethrough: '~~',
  code: '`',
}

/* ------------------------------------------------------------ source lines */

function sourceLines(text: string, from: number): Array<{ ls: number; line: string }> {
  const out: Array<{ ls: number; line: string }> = []
  let start = 0
  for (;;) {
    const nl = text.indexOf('\n', start)
    if (nl === -1) {
      out.push({ ls: start, line: text.slice(start) })
      break
    }
    out.push({ ls: start, line: text.slice(start, nl) })
    start = nl + 1
  }
  return out.filter((entry) => entry.ls + entry.line.length > from || entry.ls >= from)
}

function isFrontmatterDelimiter(line: string): boolean {
  return line === '---' || line === '---\r'
}

/**
 * A leading, properly terminated frontmatter block: `{ from: 0, to }` covering
 * both delimiters and the closing line's newline. The core's strict rule.
 */
export function frontmatterRange(text: string): Range | null {
  const firstEnd = text.indexOf('\n')
  if (firstEnd === -1) return null
  if (!isFrontmatterDelimiter(text.slice(0, firstEnd))) return null

  let pos = firstEnd + 1
  for (;;) {
    const nl = text.indexOf('\n', pos)
    const lineEnd = nl === -1 ? text.length : nl
    if (isFrontmatterDelimiter(text.slice(pos, lineEnd))) {
      return { from: 0, to: nl === -1 ? lineEnd : lineEnd + 1 }
    }
    if (nl === -1) return null
    pos = nl + 1
  }
}

/* ------------------------------------------------------------ code helpers */

const FENCE_OPEN = /^ {0,3}(`{3,}|~{3,})(.*)$/
const FENCE_CLOSE = /^ {0,3}(`{3,}|~{3,})[ \t]*$/

/** Inline code spans on one line, as `[start, end)` pairs covering the backticks. */
export function inlineCodeSpans(line: string): Array<[number, number]> {
  const spans: Array<[number, number]> = []
  const n = line.length
  let i = 0
  while (i < n) {
    if (line[i] !== '`') {
      i++
      continue
    }
    let j = i
    while (j < n && line[j] === '`') j++
    const runLen = j - i
    let k = j
    let closed = -1
    while (k < n) {
      if (line[k] === '`') {
        let m = k
        while (m < n && line[m] === '`') m++
        if (m - k === runLen) {
          closed = m
          break
        }
        k = m
      } else {
        k++
      }
    }
    if (closed > 0) {
      spans.push([i, closed])
      i = closed
    } else {
      i = j
    }
  }
  return spans
}

export function inRanges(index: number, ranges: Range[]): boolean {
  return ranges.some((range) => index >= range.from && index < range.to)
}

function overlaps(from: number, to: number, ranges: Range[]): boolean {
  return ranges.some((range) => from < range.to && to > range.from)
}

/** Every line outside a fenced code block and outside leading frontmatter. */
export function contentLines(
  text: string,
  frontmatterTo: number,
): Array<{ ls: number; le: number; line: string }> {
  const raw: Array<[number, number]> = []
  let start = 0
  for (;;) {
    const nl = text.indexOf('\n', start)
    if (nl === -1) {
      raw.push([start, text.length])
      break
    }
    raw.push([start, nl])
    start = nl + 1
  }

  let fence: { marker: string; len: number } | null = null
  const out: Array<{ ls: number; le: number; line: string }> = []
  for (const [ls, le] of raw) {
    if (frontmatterTo > 0 && ls < frontmatterTo) continue
    const line = text.slice(ls, le)

    if (fence !== null) {
      const m = FENCE_OPEN.exec(line)
      if (m !== null && FENCE_CLOSE.test(line)) {
        const marker = m[1]?.[0] ?? ''
        const len = m[1]?.length ?? 0
        if (marker === fence.marker && len >= fence.len) fence = null
      }
      continue
    }

    const m = FENCE_OPEN.exec(line)
    if (m !== null) {
      const marker = m[1]?.[0] ?? ''
      const info = m[2] ?? ''
      if (!(marker === '`' && info.includes('`'))) {
        fence = { marker, len: m[1]?.length ?? 0 }
        continue
      }
    }
    out.push({ ls, le, line })
  }
  return out
}

function isEscaped(text: string, index: number): boolean {
  let count = 0
  for (let i = index - 1; i >= 0 && text[i] === '\\'; i--) count++
  return count % 2 === 1
}

/* --------------------------------------------------------------- wikilinks */

export const NOT_LINK = {
  EMPTY_TARGET: 'empty-target',
  SAME_NOTE: 'same-note',
  EMBED: 'embed',
  UNCLOSED: 'unclosed',
  IN_CODE: 'in-code',
} as const

export type WikilinkParse =
  | { kind: 'link'; target: string; fragment?: string; alias?: string; display: string }
  | { kind: 'not-link'; reason: string; target: string }

/** `[[target[#fragment][|alias]]]`, split first at `#` then at `|`. */
export function parseWikilink(content: string): WikilinkParse {
  const hash = content.indexOf('#')
  const pipe = content.indexOf('|')
  const targetEnd = Math.min(
    hash === -1 ? content.length : hash,
    pipe === -1 ? content.length : pipe,
  )
  const target = content.slice(0, targetEnd)
  const present = (value: string): string | undefined =>
    value.trim() === '' ? undefined : value

  let fragment: string | undefined
  let alias: string | undefined
  if (hash !== -1 && pipe !== -1) {
    if (hash < pipe) {
      fragment = present(content.slice(hash + 1, pipe))
      alias = present(content.slice(pipe + 1))
    } else {
      alias = present(content.slice(pipe + 1, hash))
      fragment = present(content.slice(hash + 1))
    }
  } else if (hash !== -1) {
    fragment = present(content.slice(hash + 1))
  } else if (pipe !== -1) {
    alias = present(content.slice(pipe + 1))
  }

  if (target.trim() === '') {
    const reason =
      fragment !== undefined || alias !== undefined ? NOT_LINK.SAME_NOTE : NOT_LINK.EMPTY_TARGET
    return { kind: 'not-link', reason, target }
  }

  return {
    kind: 'link',
    target,
    ...(fragment !== undefined ? { fragment } : {}),
    ...(alias !== undefined ? { alias } : {}),
    display: alias ?? fragment ?? target,
  }
}

export type WikilinkMatch = Range & {
  content: string
  target: string
  fragment?: string
  alias?: string
  display: string
}

/** Every real wikilink in `text`, line-bounded, with masks honoured. */
export function scanWikilinks(text: string, masks: Range[] = []): WikilinkMatch[] {
  const links: WikilinkMatch[] = []
  const frontmatter = frontmatterRange(text)

  for (const { ls, line } of contentLines(text, frontmatter ? frontmatter.to : 0)) {
    const spans = inlineCodeSpans(line).map(([from, to]) => ({ from, to }))
    let i = 0
    const len = line.length
    while (i < len) {
      const at = line.indexOf('[[', i)
      if (at === -1) break
      const absolute = ls + at
      if (inRanges(at, spans) || inRanges(absolute, masks) || isEscaped(line, at)) {
        i = at + 2
        continue
      }
      if (at > 0 && line[at - 1] === '!') {
        i = at + 2
        continue
      }
      const close = line.indexOf(']]', at + 2)
      if (close === -1) {
        i = at + 2
        continue
      }
      const candidateTo = close + 2
      if (
        masks.some((range) => absolute < range.to && ls + candidateTo > range.from) ||
        spans.some((range) => at < range.to && candidateTo > range.from)
      ) {
        i = candidateTo
        continue
      }
      const content = line.slice(at + 2, close)
      const parsed = parseWikilink(content)
      if (parsed.kind === 'link') {
        links.push({
          from: absolute,
          to: ls + close + 2,
          content,
          target: parsed.target,
          ...(parsed.fragment !== undefined ? { fragment: parsed.fragment } : {}),
          ...(parsed.alias !== undefined ? { alias: parsed.alias } : {}),
          display: parsed.display,
        })
      }
      i = close + 2
    }
  }
  return links
}

/* -------------------------------------------------------------------- math */

const SINGLE_LINE_DISPLAY = /^ {0,3}\$\$([^\s$](?:[^$]*[^\s$])?)\$\$\s*$/

export type MathMatch = Range & { kind: 'inline' | 'display'; content: string }

function isFreeDollar(line: string, index: number): boolean {
  const prev = index > 0 ? line[index - 1] : ''
  const next = index + 1 < line.length ? line[index + 1] : ''
  return prev !== '$' && next !== '$'
}

function indentOf(line: string): number {
  return line.length - line.trimStart().length
}

function inlineMath(
  line: string,
  ls: number,
  spans: Range[],
  masks: Range[],
): MathMatch[] {
  const found: MathMatch[] = []
  const n = line.length
  for (let i = 0; i < n; i++) {
    if (line[i] !== '$' || !isFreeDollar(line, i)) continue
    if (inRanges(i, spans) || inRanges(ls + i, masks)) continue
    for (let j = i + 1; j < n; j++) {
      if (line[j] !== '$' || !isFreeDollar(line, j)) continue
      if (inRanges(j, spans) || inRanges(ls + j, masks)) continue
      const first = line[i + 1]
      const last = line[j - 1]
      if (first === ' ' || last === ' ') continue
      found.push({
        kind: 'inline',
        from: ls + i,
        to: ls + j + 1,
        content: line.slice(i + 1, j),
      })
      i = j
      break
    }
  }
  return found
}

/** Every math range in `text`, with the settled trigger rules and masks honoured. */
export function scanMath(text: string, masks: Range[] = []): MathMatch[] {
  const frontmatter = frontmatterRange(text)
  const lines = contentLines(text, frontmatter ? frontmatter.to : 0)
  const out: MathMatch[] = []
  const consumed = new Set<number>()

  for (let li = 0; li < lines.length; li++) {
    const entry = lines[li]
    if (entry === undefined || consumed.has(li)) continue
    const { ls, line } = entry

    if (line.trim() === '$$') {
      if (inRanges(ls + indentOf(line), masks)) continue
      for (let lj = li + 1; lj < lines.length; lj++) {
        const closer = lines[lj]
        if (closer === undefined) continue
        if (closer.line.trim() === '$$') {
          const openEnd = ls + indentOf(line) + 2
          const closeStart = closer.ls + indentOf(closer.line)
          out.push({
            kind: 'display',
            from: openEnd - 2,
            to: closeStart + 2,
            content: text.slice(openEnd, closeStart).trim(),
          })
          for (let k = li; k <= lj; k++) consumed.add(k)
          break
        }
      }
      continue
    }

    const match = SINGLE_LINE_DISPLAY.exec(line)
    if (match !== null && !inRanges(ls + indentOf(line), masks)) {
      const from = ls + indentOf(line)
      const content = match[1] ?? ''
      out.push({ kind: 'display', from, to: from + 2 + content.length + 2, content })
      consumed.add(li)
    }
  }

  const displayRanges = out.map((entry) => ({ from: entry.from, to: entry.to }))
  for (let li = 0; li < lines.length; li++) {
    const entry = lines[li]
    if (entry === undefined || consumed.has(li)) continue
    const { ls, line } = entry
    const spans = inlineCodeSpans(line).map(([from, to]) => ({ from, to }))
    for (const found of inlineMath(line, ls, spans, masks)) {
      if (displayRanges.some((range) => found.from >= range.from && found.to <= range.to)) continue
      out.push(found)
    }
  }

  out.sort((a, b) => a.from - b.from)
  return out
}

/* --------------------------------------------------------------- links */

type LinkMatch = Range & { text: string; href: string }

function scanLinks(text: string, blocked: Range[]): LinkMatch[] {
  const found: LinkMatch[] = []
  for (const { ls, line } of sourceLines(text, 0)) {
    if (line.trimStart().startsWith('```') || line.trimStart().startsWith('~~~')) continue
    const spans = inlineCodeSpans(line).map(([from, to]) => ({ from, to }))
    const pattern = /\[([^[\]\n]*)\]\(([^()\s]+)\)/g
    let match: RegExpExecArray | null
    while ((match = pattern.exec(line)) !== null) {
      const at = match.index
      const to = at + match[0].length
      const absolute = ls + at
      if (isEscaped(line, at) || inRanges(at, spans) || overlaps(ls + at, ls + to, blocked)) {
        continue
      }
      found.push({ from: absolute, to: ls + to, text: match[1] ?? '', href: match[2] ?? '' })
    }
  }
  return found
}

/* --------------------------------------------------------- document scan */

/** The fixed point the editor bundle's `scanDocument` pins. */
export function scanInlineDocument(text: string): {
  frontmatter: Range | null
  wikilinks: WikilinkMatch[]
  math: MathMatch[]
} {
  let wikilinks = scanWikilinks(text)
  let math = scanMath(
    text,
    wikilinks.map((link) => ({ from: link.from, to: link.to })),
  )
  wikilinks = scanWikilinks(
    text,
    math.map((entry) => ({ from: entry.from, to: entry.to })),
  )
  math = scanMath(
    text,
    wikilinks.map((link) => ({ from: link.from, to: link.to })),
  )
  return { frontmatter: frontmatterRange(text), wikilinks, math }
}

/* -------------------------------------------------------------- emphasis */

type Pair = {
  style: InlineStyle
  from: number
  to: number
  /** The exact opener and closer the source wrote (`_` stays `_`). */
  open: string
  close: string
}

type OpenLayer = {
  style: InlineStyle
  len: number
  contentStart: number
  markerStart: number
}

type Emphasis = { pairs: Pair[]; markers: Range[] }

/** A Unicode letter or digit, the CommonMark word test for the `_` rule. */
const WORD = /[\p{L}\p{N}]/u

function isWord(char: string): boolean {
  return char !== '' && WORD.test(char)
}

/**
 * Pair the `*`, `_` and `~~` runs outside the blocked ranges.
 *
 * A run opens layers from the outside in (`***x***` is bold then italic) and a
 * matching run closes the open layers from the inside out, so the pair ranges
 * stay nested and a run's style set is always a subset of its parent's. A pair
 * records the exact delimiters the source wrote, and only the markers of a
 * pair are skipped by the run builder; an unmatched opener stays literal text.
 *
 * CommonMark's intraword `_` rule: an underscore run whose neighbours are both
 * word characters is literal, so `snake_case` is never emphasis.
 */
function emphasisPairs(text: string, blocked: Range[]): Emphasis {
  const pairs: Pair[] = []
  const markers: Range[] = []
  const stacks: Record<'*' | '_' | '~', OpenLayer[]> = { '*': [], _: [], '~': [] }

  let i = 0
  while (i < text.length) {
    if (inRanges(i, blocked) || isEscaped(text, i)) {
      i++
      continue
    }
    const ch = text[i]
    if (ch !== '*' && ch !== '_' && ch !== '~') {
      i++
      continue
    }
    let j = i
    while (j < text.length && text[j] === ch) j++
    const len = j - i
    if (ch === '~' && len < 2) {
      i = j
      continue
    }

    const before = i > 0 ? text[i - 1] ?? '' : ''
    const after = j < text.length ? text[j] ?? '' : ''
    const canOpen = after !== '' && !/\s/.test(after)
    const canClose = before !== '' && !/\s/.test(before)

    if (ch === '_' && isWord(before) && isWord(after)) {
      // Intraword underscore: literal, not an opener or a closer.
      i = j
      continue
    }

    if (ch === '~') {
      const stack = stacks['~']
      if (canClose && stack.length > 0) {
        const layer = stack.pop()
        if (layer !== undefined) {
          pairs.push({
            style: 'strikethrough',
            from: layer.contentStart,
            to: i,
            open: text.slice(layer.markerStart, layer.markerStart + 2),
            close: text.slice(i, i + 2),
          })
          markers.push({ from: layer.markerStart, to: layer.markerStart + 2 })
          markers.push({ from: i, to: i + 2 })
        }
      } else if (canOpen) {
        stack.push({ style: 'strikethrough', len: 2, contentStart: j, markerStart: i })
      }
      i = j
      continue
    }

    const stack = stacks[ch]
    let remaining = len
    if (canClose && stack.length > 0) {
      while (remaining > 0 && stack.length > 0) {
        const layer = stack[stack.length - 1]
        if (layer === undefined || layer.len > remaining) break
        stack.pop()
        const closeStart = i + (len - remaining)
        pairs.push({
          style: layer.style,
          from: layer.contentStart,
          to: i,
          open: text.slice(layer.markerStart, layer.markerStart + layer.len),
          close: text.slice(closeStart, closeStart + layer.len),
        })
        markers.push({ from: layer.markerStart, to: layer.markerStart + layer.len })
        markers.push({ from: closeStart, to: closeStart + layer.len })
        remaining -= layer.len
      }
    }
    if (canOpen && remaining > 0) {
      let offset = len - remaining
      while (remaining >= 2) {
        stack.push({ style: 'bold', len: 2, contentStart: j, markerStart: i + offset })
        offset += 2
        remaining -= 2
      }
      if (remaining === 1) {
        stack.push({ style: 'italic', len: 1, contentStart: j, markerStart: i + offset })
      }
    }
    i = j
  }
  return { pairs, markers }
}

/* ------------------------------------------------------------------ runs */

function styleSetAt(pairs: Pair[], position: number): InlineStyle[] {
  const set = new Set<InlineStyle>()
  for (const pair of pairs) {
    if (position >= pair.from && position < pair.to) set.add(pair.style)
  }
  return STYLE_ORDER.filter((style) => set.has(style))
}

/** The source delimiters of the styles covering `position`, if any. */
function markersAt(pairs: Pair[], position: number): InlineDelimiters | undefined {
  let markers: InlineDelimiters | undefined
  for (const pair of pairs) {
    if (position < pair.from || position >= pair.to) continue
    markers ??= {}
    markers[pair.style] = { open: pair.open, close: pair.close }
  }
  return markers
}

function sameStyles(a: InlineStyle[], b: InlineStyle[]): boolean {
  return a.length === b.length && a.every((style, index) => style === b[index])
}

type Atom =
  | { kind: 'code'; from: number; to: number; text: string; marker: string }
  | { kind: 'wikilink'; from: number; to: number; match: WikilinkMatch }
  | { kind: 'math'; from: number; to: number; match: MathMatch }
  | { kind: 'link'; from: number; to: number; match: LinkMatch }

/** Parse one block's inline source into runs. */
export function parseInline(source: string, notes: string[] = []): InlineRun[] {
  const scanned = scanInlineDocument(source)
  const codeAtoms: Atom[] = []
  for (const { ls, line } of sourceLines(source, 0)) {
    if (scanned.frontmatter !== null && ls < scanned.frontmatter.to) continue
    const spans = inlineCodeSpans(line)
    for (const [from, to] of spans) {
      let len = 0
      while (line[from + len] === '`') len++
      codeAtoms.push({
        kind: 'code',
        from: ls + from,
        to: ls + to,
        text: source.slice(ls + from + len, ls + to - len),
        marker: '`'.repeat(len),
      })
    }
  }

  const atomRanges: Range[] = [
    ...codeAtoms,
    ...scanned.wikilinks,
    ...scanned.math.map((match) => ({ from: match.from, to: match.to })),
  ]
  const links = scanLinks(source, atomRanges)

  const atoms: Atom[] = [
    ...codeAtoms,
    ...scanned.wikilinks.map((match) => ({
      kind: 'wikilink' as const,
      from: match.from,
      to: match.to,
      match,
    })),
    ...scanned.math.map((match) => ({
      kind: 'math' as const,
      from: match.from,
      to: match.to,
      match,
    })),
    ...links.map((match) => ({
      kind: 'link' as const,
      from: match.from,
      to: match.to,
      match,
    })),
  ].sort((a, b) => a.from - b.from)

  const blocked: Range[] = [
    ...atoms.map((atom) => ({ from: atom.from, to: atom.to })),
  ]
  const { pairs, markers } = emphasisPairs(source, blocked)

  const runs: InlineRun[] = []
  let buffer = ''
  let bufferFrom = 0
  let bufferStyles: InlineStyle[] = []
  let atomIndex = 0

  const flush = (to: number): void => {
    if (buffer === '') return
    runs.push({
      kind: 'text',
      text: buffer,
      styles: bufferStyles,
      markers: markersAt(pairs, bufferFrom),
      from: bufferFrom,
      to,
    })
    buffer = ''
  }

  let i = 0
  while (i < source.length) {
    const atom = atoms[atomIndex]
    if (atom !== undefined && atom.from === i) {
      flush(i)
      const styles = styleSetAt(pairs, i)
      if (atom.kind === 'code') {
        const styleSet = STYLE_ORDER.filter((style) => styles.includes(style) || style === 'code')
        runs.push({
          kind: 'text',
          text: atom.text,
          styles: styleSet,
          markers: {
            ...markersAt(pairs, atom.from),
            code: { open: atom.marker, close: atom.marker },
          },
          from: atom.from,
          to: atom.to,
        })
      } else if (atom.kind === 'link') {
        runs.push({
          kind: 'link',
          text: atom.match.text,
          href: atom.match.href,
          styles,
          markers: markersAt(pairs, atom.from),
          from: atom.from,
          to: atom.to,
        })
      } else if (atom.kind === 'wikilink') {
        const path = resolveWikilink(atom.match.target, notes)
        runs.push({
          kind: 'wikilink',
          target: atom.match.target,
          ...(atom.match.fragment !== undefined ? { fragment: atom.match.fragment } : {}),
          ...(atom.match.alias !== undefined ? { alias: atom.match.alias } : {}),
          display: atom.match.display,
          path,
          broken: path === null,
          source: source.slice(atom.from, atom.to),
          from: atom.from,
          to: atom.to,
        })
      } else {
        runs.push({
          kind: 'math',
          display: atom.match.kind === 'display',
          content: atom.match.content,
          source: source.slice(atom.from, atom.to),
          from: atom.from,
          to: atom.to,
        })
      }
      i = atom.to
      atomIndex++
      continue
    }

    if (markers.some((range) => range.from === i)) {
      flush(i)
      const marker = markers.find((range) => range.from === i)
      i = marker === undefined ? i + 1 : marker.to
      continue
    }

    const styles = styleSetAt(pairs, i)
    if (buffer !== '' && !sameStyles(styles, bufferStyles)) flush(i)
    if (buffer === '') {
      bufferFrom = i
      bufferStyles = styles
    }
    buffer += source[i]
    i++
  }
  flush(source.length)
  return runs
}

/* ------------------------------------------------------------- serialise */

/** Serialise runs with the delimiters their source wrote, canonical only as a fallback. */
export function serialiseInline(runs: InlineRun[]): string {
  let out = ''
  const open: InlineStyle[] = []
  const openMarkers = new Map<InlineStyle, InlineDelimiter>()

  const closeTo = (styles: InlineStyle[]): void => {
    for (let index = open.length - 1; index >= 0; index--) {
      const style = open[index]
      if (style === undefined || styles.includes(style)) continue
      out += openMarkers.get(style)?.close ?? DELIMITER[style]
      openMarkers.delete(style)
      open.splice(index, 1)
    }
  }
  const openTo = (styles: InlineStyle[], run: TextRun | LinkRun): void => {
    for (const style of STYLE_ORDER) {
      if (!styles.includes(style) || open.includes(style)) continue
      const delimiter = run.markers?.[style] ?? { open: DELIMITER[style], close: DELIMITER[style] }
      out += delimiter.open
      openMarkers.set(style, delimiter)
      open.push(style)
    }
  }

  for (const run of runs) {
    if (run.kind === 'text' || run.kind === 'link') {
      closeTo(run.styles)
      openTo(run.styles, run)
      out += run.kind === 'link' ? `[${run.text}](${run.href})` : run.text
      continue
    }
    if (run.kind === 'wikilink') {
      closeTo([])
      out += `[[${run.target}${run.fragment === undefined ? '' : `#${run.fragment}`}${run.alias === undefined ? '' : `|${run.alias}`}]]`
      continue
    }
    closeTo([])
    out += run.display ? `$$\n${run.content}\n$$` : `$${run.content}$`
  }
  closeTo([])
  return out
}

/** The style ranges a command may unwrap, as content offsets plus the marker. */
export function styleSpans(
  source: string,
  style: InlineStyle,
): Array<{ from: number; to: number; marker: string }> {
  if (style === 'code') {
    const spans: Array<{ from: number; to: number; marker: string }> = []
    for (const { ls, line } of sourceLines(source, 0)) {
      for (const [from, to] of inlineCodeSpans(line)) {
        let len = 0
        while (line[from + len] === '`') len++
        spans.push({
          from: ls + from + len,
          to: ls + to - len,
          marker: '`'.repeat(len),
        })
      }
    }
    return spans
  }

  const frontmatter = frontmatterRange(source)
  const blocked: Range[] = []
  for (const { ls, line } of sourceLines(source, frontmatter ? frontmatter.to : 0)) {
    if (frontmatter !== null && ls < frontmatter.to) continue
    for (const [from, to] of inlineCodeSpans(line)) blocked.push({ from: ls + from, to: ls + to })
  }
  for (const link of scanLinks(source, blocked)) blocked.push({ from: link.from, to: link.to })

  const { pairs } = emphasisPairs(source, blocked)
  return pairs
    .filter((pair) => pair.style === style)
    .map((pair) => ({ from: pair.from, to: pair.to, marker: pair.open }))
}

/* ---------------------------------------------------------------- picker */

export type NoteChoice = { path: string; label: string; insert: string }

function withoutExtension(path: string): string {
  return path.replace(/\.md$/i, '')
}

function basename(path: string): string {
  const parts = path.split('/')
  return withoutExtension(parts[parts.length - 1] ?? path)
}

/**
 * The picker's rows for a query: existing notes only, matched case-insensitively
 * on the basename and on the root-relative path. A bare basename is inserted
 * only when exactly one note carries it; a duplicated basename inserts the
 * qualified path, so the pill's exactly-one rule always holds.
 */
export function filterNotes(notes: string[], query: string): NoteChoice[] {
  const wanted = query.trim().toLowerCase()
  const counts = new Map<string, number>()
  for (const path of notes) {
    const name = basename(path).toLowerCase()
    counts.set(name, (counts.get(name) ?? 0) + 1)
  }

  const choices: NoteChoice[] = []
  for (const path of notes) {
    const name = basename(path)
    const qualified = withoutExtension(path)
    if (
      wanted !== '' &&
      !name.toLowerCase().includes(wanted) &&
      !qualified.toLowerCase().includes(wanted)
    ) {
      continue
    }
    const unique = (counts.get(name.toLowerCase()) ?? 0) === 1
    choices.push({ path, label: name, insert: unique ? name : qualified })
  }
  return choices.sort((a, b) => a.path.localeCompare(b.path))
}

/**
 * The open `[[` before `offset`, with the query typed since it, or null when
 * the caret is not inside an unclosed, unmasked wikilink opener.
 */
export function wikilinkQueryAt(
  source: string,
  offset: number,
): { from: number; query: string } | null {
  const before = source.slice(0, Math.max(0, Math.min(source.length, offset)))
  const open = before.lastIndexOf('[[')
  if (open === -1) return null
  if (before.slice(open + 2).includes(']]')) return null
  if (isEscaped(source, open)) return null
  if (open > 0 && source[open - 1] === '!') return null

  const scanned = scanInlineDocument(source)
  const protectedRanges: Range[] = [
    ...scanned.wikilinks,
    ...scanned.math,
  ]
  if (inRanges(open, protectedRanges)) return null
  for (const { ls, line } of sourceLines(source, 0)) {
    for (const [from, to] of inlineCodeSpans(line)) {
      if (open >= ls + from && open < ls + to) return null
    }
  }
  return { from: open, query: source.slice(open + 2, offset) }
}
