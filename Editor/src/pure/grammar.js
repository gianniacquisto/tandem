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

  const hash = content.indexOf("#");
  if (hash >= 0) {
    target = content.slice(0, hash);
    const rest = content.slice(hash + 1);
    const pipe = rest.indexOf("|");
    if (pipe >= 0) {
      fragment = rest.slice(0, pipe);
      alias = rest.slice(pipe + 1);
    } else {
      fragment = rest;
    }
  } else {
    const pipe = content.indexOf("|");
    if (pipe >= 0) {
      target = content.slice(0, pipe);
      alias = content.slice(pipe + 1);
    } else {
      target = content;
    }
  }

  if (target.trim() === "") {
    // `[[#Heading]]` is the same-note form; `[[#]]`, `[[|x]]`, `[[ ]]` are
    // empty/whitespace-only targets. All are literal text (spec: not links).
    const reason =
      (fragment || "").trim() !== "" || alias !== undefined
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
