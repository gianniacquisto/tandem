// Document-level composition of the pure scanners (editor layer calls this
// once per reparse).
//
// Ordering matters and is fixed-point:
//   1. wikilinks first — the grammar is independent of math;
//   2. math with the wikilink ranges masked — a wikilink target keeps its
//      text as written, so `$…$` inside `[[…]]` is not math;
//   3. re-scan wikilinks with the math ranges masked — a `[[…]]` inside a
//      display block is math source, not a link.
// Round 3 cannot remove a math range found in round 2 (math delimiters never
// sit inside a wikilink range that survives round 2), so the pair is stable.

import { scanWikilinks } from "./scan.js";
import { scanMath } from "./math.js";
import { frontmatterRange } from "./frontmatter.js";

const ranges = (items) => items.map((i) => [i.from, i.to]);

/**
 * @param {string} text raw markdown document
 * @returns {{ frontmatter: {from: number, to: number} | null,
 *             wikilinks: Array, math: Array }}
 */
export function scanDocument(text) {
  let wikilinks = scanWikilinks(text);
  let math = scanMath(text, ranges(wikilinks));
  wikilinks = scanWikilinks(text, ranges(math));
  math = scanMath(text, ranges(wikilinks));
  return {
    frontmatter: frontmatterRange(text),
    wikilinks,
    math,
  };
}
