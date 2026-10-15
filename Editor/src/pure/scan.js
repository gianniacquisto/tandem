// Wikilink scanning (spec: Wikilink model — Grammar v1; Rendered feature list
// cross-cutting invariants).
//
// `scanWikilinks(text, masks)` returns every real wikilink in a raw markdown
// document as { from, to, content, target, fragment?, alias?, display }, with
// absolute offsets into `text` (`to` exclusive, covering `[[…]]`).
//
// Rules implemented here:
// - scan left-to-right; the first `]]` closes a candidate (line-bounded: a
//   `[[` without a `]]` on the same line is an unclosed literal, per the
//   spec's "unclosed `[[` at end of line/file");
// - candidates whose target parses as a not-link (empty/whitespace, same-note
//   form) are dropped — the parse lives in grammar.js;
// - `![[…]]` embeds are literal (the `!` immediately before `[[` disqualifies);
// - nothing is processed inside fenced code blocks, inline code spans, a
//   leading frontmatter block, or any caller-supplied mask range (the editor
//   masks math ranges, which are found after the wikilink pass — a wikilink
//   target keeps its text as written);
//
// `masks` is an array of [from, to) ranges never scanned.
// Pure: no DOM.

import { parseWikilink } from "./grammar.js";
import { frontmatterRange } from "./frontmatter.js";
import { contentLines, inlineCodeSpans, inRanges } from "./code.js";

/**
 * Returns the wikilink candidates (real links only) in `text`.
 */
export function scanWikilinks(text, masks = []) {
  const links = [];
  const fm = frontmatterRange(text);

  for (const { ls, line } of contentLines(text, fm ? fm.to : 0)) {
    const spans = inlineCodeSpans(line);
    let i = 0;
    const len = line.length;
    while (i < len) {
      const at = line.indexOf("[[", i);
      if (at === -1) break;
      if (inRanges(at, spans) || inRanges(ls + at, masks)) {
        i = at + 2;
        continue;
      }
      if (at > 0 && line[at - 1] === "!") {
        i = at + 2; // embed: literal
        continue;
      }
      const closeIdx = line.indexOf("]]", at + 2);
      if (closeIdx === -1) {
        i = at + 2; // unclosed on this line: literal, keep scanning
        continue;
      }
      const content = line.slice(at + 2, closeIdx);
      const parsed = parseWikilink(content);
      if (parsed.kind === "link") {
        links.push({
          from: ls + at,
          to: ls + closeIdx + 2,
          content,
          target: parsed.target,
          ...(parsed.fragment !== undefined ? { fragment: parsed.fragment } : {}),
          ...(parsed.alias !== undefined ? { alias: parsed.alias } : {}),
          display: parsed.display,
        });
      }
      i = closeIdx + 2; // the first ]] closed this candidate
    }
  }
  return links;
}
