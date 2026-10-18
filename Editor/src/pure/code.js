// Shared code-context helpers (spec cross-cutting invariant: "nothing
// special — wikilink, math, image — is processed inside code fences or
// inline code"). Pure: no DOM.
//
// Fence tracking follows the GFM/CommonMark rules the bikeshed index uses: a
// fence opens with ≥3 backticks or tildes (up to 3 leading spaces); a
// backtick fence's info string may not contain backticks; a fence closes with
// a line of the same marker, at least as long, and no info string.

const FENCE_OPEN = /^ {0,3}(`{3,}|~{3,})(.*)$/;
const FENCE_CLOSE = /^ {0,3}(`{3,}|~{3,})[ \t]*$/;

/**
 * Inline code spans on one line as [start, end) pairs covering both backtick
 * runs. A run of N backticks closes only at the next maximal run of exactly N
 * backticks; an unmatched opener is literal text.
 */
export function inlineCodeSpans(line) {
  const spans = [];
  const n = line.length;
  let i = 0;
  while (i < n) {
    if (line[i] !== "`") {
      i++;
      continue;
    }
    let j = i;
    while (j < n && line[j] === "`") j++;
    const runLen = j - i;
    let k = j;
    let closed = -1;
    while (k < n) {
      if (line[k] === "`") {
        let m = k;
        while (m < n && line[m] === "`") m++;
        if (m - k === runLen) {
          closed = m;
          break;
        }
        k = m; // a longer run cannot close this one
      } else {
        k++;
      }
    }
    if (closed > 0) {
      spans.push([i, closed]);
      i = closed;
    } else {
      i = j; // unmatched opener: the run is literal text
    }
  }
  return spans;
}

/**
 * True when `idx` falls inside one of the [start, end) ranges.
 */
export function inRanges(idx, ranges) {
  for (const [s, e] of ranges) {
    if (idx >= s && idx < e) return true;
  }
  return false;
}

/**
 * Every line of `text` outside fenced code blocks and outside a leading
 * frontmatter block, as { ls, le, line } with absolute offsets (le exclusive;
 * `line` is the text without its newline). Fence state is tracked across
 * lines; frontmatter lines are skipped entirely (they never track fences).
 */
export function contentLines(text, frontmatterTo) {
  const lines = [];
  let start = 0;
  for (;;) {
    const nl = text.indexOf("\n", start);
    if (nl === -1) {
      lines.push([start, text.length]);
      break;
    }
    lines.push([start, nl]);
    start = nl + 1;
  }

  let fence = null; // { marker, len } while inside a fenced block
  const out = [];
  for (const [ls, le] of lines) {
    if (frontmatterTo > 0 && ls < frontmatterTo) continue;

    const line = text.slice(ls, le);

    if (fence) {
      const m = FENCE_OPEN.exec(line);
      if (m && FENCE_CLOSE.test(line)) {
        const marker = m[1][0];
        if (marker === fence.marker && m[1].length >= fence.len) fence = null;
      }
      continue;
    }

    const m = FENCE_OPEN.exec(line);
    if (m) {
      const marker = m[1][0];
      if (!(marker === "`" && m[2].includes("`"))) {
        fence = { marker, len: m[1].length };
        continue; // the fence line itself is not content
      }
    }
    out.push({ ls, le, line });
  }
  return out;
}
