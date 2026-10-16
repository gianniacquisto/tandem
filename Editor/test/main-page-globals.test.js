import { test } from "node:test";
import assert from "node:assert/strict";

import { NodeProp } from "@lezer/common";
import { syntaxTree } from "@codemirror/language";
import {
  createEditorState,
  livePreviewState,
  vaultNotesState,
  notePathState,
  NOTE_CEILING,
  CEILING_WARNING,
  setDocumentSpec,
  insertTextSpec,
  revealLineSpec,
  setVaultIndexSpec,
  dirtyTextForUpdate,
} from "../src/main.js";
import { CeilingWarningWidget, ImageWidget } from "../src/widgets.js";

// The Swift → JS page globals, headless: each one is a pure TransactionSpec
// builder (main.js), so applying it to a plain EditorState exercises the
// whole effect — the guarded layer only dispatches the same spec from the
// window global. No DOM anywhere in this file.

function decos(state) {
  const out = [];
  const cur = state.field(livePreviewState).iter();
  while (cur.value) {
    out.push({ from: cur.from, to: cur.to, spec: cur.value.spec });
    cur.next();
  }
  return out;
}

const blockAt = (ds, at) =>
  ds.find((d) => d.from === d.to && d.from === at && d.spec.widget);
const widgetReplace = (ds, from) =>
  ds.find((d) => d.from === from && d.spec.widget)?.spec.widget;

// -- setDocument ---------------------------------------------------------------------

test("setDocument replaces the document, sets the note path, emits no dirty", () => {
  const st = createEditorState({ doc: "old text", notePath: "old/n.md" });
  const tr = st.update(setDocumentSpec(st, "# Fresh", "new/n.md"));
  const next = tr.state;
  assert.equal(next.doc.toString(), "# Fresh");
  assert.equal(next.field(notePathState), "new/n.md");
  assert.equal(next.selection.main.head, 0);
  // Replacing the document is not a user edit: nothing is posted.
  assert.equal(dirtyTextForUpdate([tr], next), null);
});

test("setDocument on a frontmatter note still plans the tree walk's decorations", () => {
  // The exact app sequence (empty page, then setDocument) with a note that
  // carries frontmatter: the plan field must not be left with only the
  // pure-scanner decorations (regression: the visit guard skipped the
  // Document root and the note rendered as raw source).
  const st0 = createEditorState({ doc: "" });
  const tr = st0.update(
    setDocumentSpec(st0, "---\nt: x\n---\n# After\n", "n.md")
  );
  const next = tr.state;
  const ds = decos(next);
  assert.ok(ds.some((d) => d.spec.class === "tandem-h1"));
  assert.ok(ds.some((d) => d.from === 13 && d.to === 15 && !d.spec.widget));
});

// -- insertText ----------------------------------------------------------------------

test("insertText inserts at the caret, moves it, and dirties", () => {
  let st = createEditorState({ doc: "ab cd" });
  st = st.update({ selection: { anchor: 3 } }).state; // caret before "cd"
  const tr = st.update(insertTextSpec(st, "XY"));
  st = tr.state;
  assert.equal(st.doc.toString(), "ab XYcd");
  assert.equal(st.selection.main.head, 5);
  // A document change: dirty carries the full raw text.
  assert.equal(dirtyTextForUpdate([tr], st), "ab XYcd");
});

// -- revealLine ----------------------------------------------------------------------

test("revealLine moves the caret to the line start and scrolls it into view", () => {
  const st = createEditorState({ doc: "one\ntwo\nthree" });
  const tr = st.update(revealLineSpec(st, 2));
  assert.equal(tr.state.selection.main.head, 4); // "two".from
  const target = tr.effects[0].value;
  assert.equal(target.range.anchor, 4);
  assert.equal(target.range.head, 4);
  assert.equal(target.y, "start");
});

test("revealLine clamps out-of-range lines to the document", () => {
  const st = createEditorState({ doc: "one\ntwo\nthree" });
  assert.equal(revealLineSpec(st, 99).selection.anchor, 8); // last line
  assert.equal(revealLineSpec(st, 0).selection.anchor, 0); // first line
  assert.equal(revealLineSpec(st, -5).selection.anchor, 0);
  // An empty document is one empty line.
  const empty = createEditorState({ doc: "" });
  assert.equal(revealLineSpec(empty, 7).selection.anchor, 0);
});

// -- setVaultIndex -------------------------------------------------------------------

test("setVaultIndex updates the index field and re-resolves links, doc untouched", () => {
  const st = createEditorState({ doc: "[[note]]" });
  const tr = st.update(setVaultIndexSpec(["note.md"]));
  const next = tr.state;
  assert.equal(tr.docChanged, false);
  assert.deepEqual(next.field(vaultNotesState), ["note.md"]);
  const w = widgetReplace(decos(next), 0);
  assert.ok(w);
  assert.equal(w.broken, false);
});

// -- Soft per-note ceiling -------------------------------------------------------------

test("a note past the soft ceiling shows the warning row", () => {
  const st = createEditorState({ doc: "x".repeat(NOTE_CEILING + 1) });
  const w = blockAt(decos(st), 0);
  assert.ok(w.spec.widget instanceof CeilingWarningWidget);
  assert.equal(w.spec.widget.message, CEILING_WARNING);
});

test("a note exactly at the ceiling does not warn (the bound is soft, not strict)", () => {
  const st = createEditorState({ doc: "x".repeat(NOTE_CEILING) });
  assert.equal(blockAt(decos(st), 0), undefined);
});

// -- Image targets (remote rewrite, vault resolution) ----------------------------------

test("a remote image carries a remote target (never fetched)", () => {
  const st = createEditorState({ doc: "![a](https://example.com/p.png)" });
  const img = blockAt(decos(st), 0);
  assert.ok(img.spec.widget instanceof ImageWidget);
  assert.equal(img.spec.widget.target.kind, "remote");
});

test("a local image resolves against the note's folder in the vault", () => {
  const st = createEditorState({ doc: "![alt](img/p.png)", notePath: "a/n.md" });
  const img = blockAt(decos(st), 0);
  assert.ok(img.spec.widget instanceof ImageWidget);
  assert.equal(img.spec.widget.target.kind, "vault");
  assert.equal(img.spec.widget.target.path, "a/img/p.png");
});

// -- Curated code highlighting -----------------------------------------------------------

// The nested parser for a fenced block is the FencedCode node's
// NodeProp.mounted prop: a MountedTree when the language is in the curated
// set, none otherwise (plain monospace — spec: Rendered feature list).
function codeIsMounted(info) {
  const st = createEditorState({ doc: "```" + info + "\nhello\n```" });
  const cur = syntaxTree(st).cursor();
  do {
    const n = cur.node;
    if (n.name === "FencedCode") return !!n.prop(NodeProp.mounted);
  } while (cur.next());
  assert.fail("no FencedCode node for ```" + info);
}

test("fenced code highlights exactly the curated languages", () => {
  // js/jsx + typescript/ts/tsx come from the one pinned
  // @codemirror/lang-javascript (its `typescript` flag).
  for (const info of ["css", "js", "jsx", "typescript", "ts", "tsx", "md"]) {
    assert.equal(codeIsMounted(info), true, "```" + info + " is highlighted");
  }
  // No pinned language package: swift, shell, yaml, toml stay plain
  // monospace (adding deps would break the pinned-package + license gate).
  for (const info of ["swift", "no-such-lang", ""]) {
    assert.equal(codeIsMounted(info), false, "```" + info + " is plain monospace");
  }
});
