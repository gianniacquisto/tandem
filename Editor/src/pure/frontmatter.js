// Leading YAML frontmatter detection (spec: Vault protocol — Frontmatter).
//
// The core's STRICT rule (BikeshedOperations.frontmatterEnd): only a
// *leading, properly terminated* block of EXACT `---` delimiters counts —
// the document's first line is exactly `---` and a later line is exactly
// `---`. No trailing whitespace, no `...` closer (a `...`-terminated block
// is plain document text — wikilinks in it stay live, so the core's
// rename-rewrite and the editor's opaque-frontmatter view agree). A lone
// trailing `\r` is tolerated (CRLF, core parity). An unterminated block is
// not frontmatter — its lines are plain document text. Pure: no DOM.

function isDelimiter(line) {
  return line === "---" || line === "---\r";
}

/**
 * Returns { from: 0, to } covering the frontmatter block including both
 * delimiter lines and the trailing newline of the closing line, or null when
 * the document has no leading properly-terminated frontmatter block.
 */
export function frontmatterRange(text) {
  const firstEnd = text.indexOf("\n");
  if (firstEnd === -1) return null; // a lone `---` is not a terminated block
  const firstLine = text.slice(0, firstEnd);
  if (!isDelimiter(firstLine)) return null;

  // Scan the lines after the opening delimiter for the terminator.
  let pos = firstEnd + 1;
  for (;;) {
    const nl = text.indexOf("\n", pos);
    const lineEnd = nl === -1 ? text.length : nl;
    const line = text.slice(pos, lineEnd);
    if (isDelimiter(line)) {
      return { from: 0, to: nl === -1 ? lineEnd : lineEnd + 1 };
    }
    if (nl === -1) return null;
    pos = nl + 1;
  }
}
