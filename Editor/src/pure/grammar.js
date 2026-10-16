// Pure wikilink grammar parsing (spec: Wikilink model — Grammar v1).
//
// `parseWikilink(content)` parses the text between `[[` and the first `]]`
// (the scanner finds those; see scan.js). It never touches the DOM.
//
// Grammar: `[[target[#fragment][|alias]]]` — split at the first `#`, then the
// first `|`. Display text = alias if present, else fragment if present, else
// the target as written.

export const NOT_LINK = {
  EMPTY_TARGET: "empty-target",
  SAME_NOTE: "same-note",
  EMBED: "embed",
  UNCLOSED: "unclosed",
  IN_CODE: "in-code",
};

export function parseWikilink(content) {
  let target;
  let fragment;
  let alias;

  // Core parity (Wikilink.init): the split is first-#-then-first-| on the
  // WHOLE content, regardless of textual order. The target runs to whichever
  // delimiter comes first; the earlier delimiter introduces the fragment,
  // the later one the alias (so `a|b#c` is target a, alias b, fragment c —
  // never target `a|b`, which could not resolve).
  const hash = content.indexOf("#");
  const pipe = content.indexOf("|");
  const targetEnd = Math.min(
    hash === -1 ? content.length : hash,
    pipe === -1 ? content.length : pipe
  );
  target = content.slice(0, targetEnd);
  // Core parity (Wikilink.present): an empty or whitespace-only fragment or
  // alias counts as ABSENT — the display text falls back to the next-present
  // part (`[[a|]]` displays `a`, never an empty link).
  const present = (s) => (s.trim() === "" ? undefined : s);
  if (hash !== -1 && pipe !== -1) {
    if (hash < pipe) {
      fragment = present(content.slice(hash + 1, pipe));
      alias = present(content.slice(pipe + 1));
    } else {
      alias = present(content.slice(pipe + 1, hash));
      fragment = present(content.slice(hash + 1));
    }
  } else if (hash !== -1) {
    fragment = present(content.slice(hash + 1));
  } else if (pipe !== -1) {
    alias = present(content.slice(pipe + 1));
  }

  if (target.trim() === "") {
    // `[[#Heading]]` is the same-note form; `[[#]]`, `[[|x]]`, `[[ ]]` are
    // empty/whitespace-only targets. All are literal text (spec: not links).
    const reason =
      fragment !== undefined || alias !== undefined
        ? NOT_LINK.SAME_NOTE
        : NOT_LINK.EMPTY_TARGET;
    return { kind: "not-link", reason, target };
  }

  return {
    kind: "link",
    target,
    ...(fragment !== undefined ? { fragment } : {}),
    ...(alias !== undefined ? { alias } : {}),
    display: alias !== undefined ? alias : fragment !== undefined ? fragment : target,
  };
}
