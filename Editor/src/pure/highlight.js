// Highlight mark scanning (TAND-10: GitHub-style ==highlight== marks,
// Obsidian trigger rules).
//
// `scanHighlights(text, masks)` returns every highlight range in a raw
// markdown document as { from, to, content } with absolute offsets (`to`
// exclusive, covering both `==` delimiters). `content` is the source between
// the delimiters (what the mark span renders).
//
// Trigger rules (mirroring the math inline rules):
// - SAME LINE: the opener and closer are `==` runs of exactly two (a `===`
//   run is literal, neither delimiter may touch another `=`), with
//   non-empty content whose edges are non-space (the character after the
//   opener and the character before the closer are not spaces): `== hi==`
//   and `==hi ==` stay literal, and an unmatched opener is literal.
// - The first valid closer wins; the scan resumes after it.
// - Nothing is processed inside fenced code blocks, inline code spans, a
//   leading frontmatter block, or any caller-supplied mask range (the editor
//   passes the tree's indented-code ranges). A candidate whose range
//   overlaps a code span is literal (core parity: the Swift scanner drops
//   any token whose range overlaps a code span, not only tokens that start
//   inside one). Overlaps with wikilink and math ranges are resolved by the
//   plan's cleanup (the mark is a span: a span strictly inside a replaced
//   range is dropped, one merely crossing it survives, so a mark wraps a
//   link while a link target keeps its text).
//
// Pure: no DOM.

import { contentLines, inlineCodeSpans, inRanges } from "./code.js";
import { frontmatterRange } from "./frontmatter.js";

// A `==` delimiter at i (two chars): neither side may touch another `=`.
function isFreeEquals(line, i) {
  const prev = i > 0 ? line[i - 1] : "";
  const next = i + 2 < line.length ? line[i + 2] : "";
  return prev !== "=" && next !== "=";
}

function highlightsOnLine(line, ls, spans, masks) {
  const found = [];
  const n = line.length;
  let i = 0;
  while (i < n - 1) {
    if (line[i] !== "=" || line[i + 1] !== "=" || !isFreeEquals(line, i)) {
      i++;
      continue;
    }
    if (inRanges(i, spans) || inRanges(ls + i, masks)) {
      i++;
      continue;
    }
    // The first `==` run at or after the content start closes the
    // candidate; an edge-rule failure keeps searching (math parity), an
    // overlap with hidden source abandons it (scan parity).
    let next = i + 2;
    for (let j = i + 2; j < n - 1; j++) {
      if (line[j] !== "=" || line[j + 1] !== "=" || !isFreeEquals(line, j)) continue;
      if (j === i + 2) continue; // empty content
      const first = line[i + 2];
      const last = line[j - 1];
      if (first === " " || last === " ") continue; // edge rule
      if (
        masks.some(([s, e]) => ls + i < e && ls + j + 2 > s) ||
        spans.some(([s, e]) => i < e && j + 2 > s)
      ) {
        next = j + 2; // overlaps hidden source: literal, resume after it
        break;
      }
      found.push({ from: ls + i, to: ls + j + 2, content: line.slice(i + 2, j) });
      next = j + 2; // resume after the closer
      break;
    }
    i = next;
  }
  return found;
}

/**
 * Returns the highlight ranges (both delimiters included) in `text`.
 */
export function scanHighlights(text, masks = []) {
  const out = [];
  const fm = frontmatterRange(text);
  for (const { ls, line } of contentLines(text, fm ? fm.to : 0)) {
    const spans = inlineCodeSpans(line);
    for (const h of highlightsOnLine(line, ls, spans, masks)) out.push(h);
  }
  return out;
}
