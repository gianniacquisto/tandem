// Wikilink resolution (spec: Wikilink model — Resolution).
//
// `resolveWikilink(target, notes)` resolves a parsed wikilink target against
// the current vault file set, exactly the rules the core `Bikeshed` resolver
// implements (single resolver semantics — editor, backlinks, rename-rewrite):
//
// - a BARE target (no `/`) matches a note's basename — the filename minus its
//   `.md` extension, compared case-insensitively — and resolves iff EXACTLY
//   ONE note matches;
// - a QUALIFIED target (contains `/`) is a vault-root-relative path, matched
//   case-insensitively against each note's vault-root-relative path with its
//   `.md` extension stripped; same exactly-one rule;
// - zero or multiple matches → broken. Duplicate basenames therefore make a
//   bare link a visible broken link, never a silently chosen match; on a
//   case-sensitive volume, case-variant duplicates make any link to that name
//   ambiguous (the exactly-one rule counts both).
// - The target is otherwise compared as written (no trimming), EXCEPT that a
//   trailing `.md` on the target is tolerated — stripped before comparison,
//   exactly as the core `NoteIndex.resolve` does (ticket 06, tested: a bare
//   or qualified target may carry a trailing `.md`). The spec is silent on
//   this case; the editor and the core must agree, and the core is the
//   single resolver (backlinks, rename-rewrite, search).
//
// `notes` is the bridge's file set: vault-root-relative paths of every NOTE,
// case as on disk (attachments are never in it — notes only are linkable).
// The resolved path is returned exactly as on disk. Pure: no DOM.

/** Path minus a trailing `.md`/`.MD`-style extension, if present. */
function stripMd(path) {
  const lower = path.toLowerCase();
  if (lower.endsWith(".md")) return path.slice(0, -3);
  return path;
}

function basename(path) {
  const i = path.lastIndexOf("/");
  return i === -1 ? path : path.slice(i + 1);
}

/**
 * @param {string} target the wikilink target as written (no brackets, no
 *   fragment/alias — those are stripped by the grammar).
 * @param {string[]} notes vault-root-relative note paths, case as on disk.
 * @returns {{status: "resolved", path: string} | {status: "broken"}}
 */
export function resolveWikilink(target, notes) {
  const qualified = target.includes("/");
  let key = target.toLowerCase();
  if (key.endsWith(".md")) key = key.slice(0, -3); // tolerate a trailing .md

  let match = null;
  let count = 0;
  for (const p of notes) {
    const noext = stripMd(p);
    const k = (qualified ? noext : basename(noext)).toLowerCase();
    if (k === key) {
      match = p;
      count++;
      if (count > 1) return { status: "broken" }; // ambiguous: stop early
    }
  }
  return count === 1 ? { status: "resolved", path: match } : { status: "broken" };
}
