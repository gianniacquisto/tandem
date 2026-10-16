// Math trigger rules (spec: Rendered feature list — KaTeX math, inline +
// display block form, Obsidian-style trigger rules). Pure: no DOM.
//
// `scanMath(text, masks)` returns every math range in a raw markdown document:
// { from, to, kind: "inline" | "display", content } with absolute offsets
// (`to` exclusive, covering the delimiters). `content` is the source between
// the delimiters (what the KaTeX renderer receives), trimmed for display.
//
// Trigger rules (settled by the research verdict):
// - INLINE: `$…$` on one line where the opener is not part of a `$$` pair and
//   the content's edges are non-space (the character after the opener and the
//   character before the closer are not spaces) — a lone currency `$` with no
//   valid closer never starts math. A `$` adjacent (before or after) to
//   another `$` is part of a `$$`/`$$$` run and never an inline delimiter.
//   The content of a single display range may not contain `$` (pathological
//   mixed runs stay literal — deterministic and simple).
// - DISPLAY: block form `$$…$$` on its own lines — either a line whose
//   (≤3-space-indented) content is exactly `$$content$$` with non-empty
//   content, or a line whose trimmed content is exactly `$$` opening a block
//   closed by a later line whose trimmed content is exactly `$$`.
// - Nothing is processed inside fenced code blocks, inline code spans, a
//   leading frontmatter block, or any caller-supplied mask range (the
//   editor passes the wikilink ranges, whose targets keep text as written).
//
// Delimiters must sit outside inline code spans; an unmatched `$$` opener or
// a `$$` line with other text is literal.

import { contentLines, inlineCodeSpans, inRanges } from "./code.js";
import { frontmatterRange } from "./frontmatter.js";

// `$$content$$` on its own line (≤3-space indent, trailing spaces allowed):
// content = one non-space non-$ char, optionally more chars ending non-space.
const SINGLE_LINE_DISPLAY = /^ {0,3}\$\$([^\s$](?:[^$]*[^\s$])?)\$\$\s*$/;

function isFreeDollar(line, i) {
  const prev = i > 0 ? line[i - 1] : "";
  const next = i + 1 < line.length ? line[i + 1] : "";
  return prev !== "$" && next !== "$";
}

function indentOf(line) {
  return line.length - line.trimStart().length;
}

function inlineMath(line, ls, spans, masks) {
  const found = [];
  const n = line.length;
  for (let i = 0; i < n; i++) {
    if (line[i] !== "$") continue;
    if (!isFreeDollar(line, i)) continue;
    if (inRanges(i, spans) || inRanges(ls + i, masks)) continue;
    // search for the earliest valid closer on the same line
    for (let j = i + 1; j < n; j++) {
      if (line[j] !== "$") continue;
      if (!isFreeDollar(line, j)) continue;
      if (inRanges(j, spans) || inRanges(ls + j, masks)) continue;
      const first = line[i + 1];
      const last = line[j - 1];
      if (first === " " || last === " ") continue; // edge rule (j > i+1 ⇒ both exist)
      found.push({
        kind: "inline",
        from: ls + i,
        to: ls + j + 1,
        content: line.slice(i + 1, j),
      });
      i = j; // resume after the closer
      break;
    }
  }
  return found;
}

export function scanMath(text, masks = []) {
  const fm = frontmatterRange(text);
  const lines = contentLines(text, fm ? fm.to : 0);
  const out = [];

  // Display pass: single-line `$$…$$` and multi-line `$$` … `$$` blocks.
  const consumed = new Set(); // line indices used by a display block
  for (let li = 0; li < lines.length; li++) {
    const { ls, line } = lines[li];
    if (consumed.has(li)) continue;

    if (line.trim() === "$$") {
      // multi-line opener (must not be masked)
      if (inRanges(ls + indentOf(line), masks)) continue;
      for (let lj = li + 1; lj < lines.length; lj++) {
        if (lines[lj].line.trim() === "$$") {
          const closer = lines[lj];
          const openEnd = ls + indentOf(line) + 2; // just past the opening $$
          const closeStart = closer.ls + indentOf(closer.line);
          out.push({
            kind: "display",
            from: openEnd - 2,
            to: closeStart + 2,
            content: text.slice(openEnd, closeStart).trim(),
          });
          for (let k = li; k <= lj; k++) consumed.add(k);
          break;
        }
      }
      continue;
    }

    const m = line.match(SINGLE_LINE_DISPLAY);
    if (m && !inRanges(ls + indentOf(line), masks)) {
      const from = ls + indentOf(line);
      out.push({
        kind: "display",
        from,
        to: from + 2 + m[1].length + 2,
        content: m[1],
      });
      consumed.add(li);
    }
  }

  // Inline pass: skip lines consumed by a display block (their text is math
  // source, not document text).
  const displayRanges = out.map((d) => [d.from, d.to]);
  for (let li = 0; li < lines.length; li++) {
    if (consumed.has(li)) continue;
    const { ls, line } = lines[li];
    const spans = inlineCodeSpans(line);
    for (const r of inlineMath(line, ls, spans, masks)) {
      if (displayRanges.some(([s, e]) => r.from >= s && r.to <= e)) continue;
      out.push(r);
    }
  }

  out.sort((a, b) => a.from - b.from);
  return out;
}
