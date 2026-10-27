import { test } from "node:test";
import assert from "node:assert/strict";

import {
  createEditorState,
  livePreviewState,
  noteIndexState,
  notePathState,
  setNoteIndexEffect,
  setNotePathEffect,
} from "../src/main.js";
import { WikilinkWidget, TableWidget, DisplayMathWidget, FrontmatterWidget } from "../src/widgets.js";

// The live-preview plan field contract: a StateField over a plain EditorState
// that recomputes the blocks.js plan from syntaxTree(state) + the bikeshed
// index + the open note path, and publishes it as a DecorationSet.
// Headless: no DOM anywhere in this file.

// Flatten the field's DecorationSet into { from, to, spec } (spec = the
// decoration's config: {class}, {class, attributes}, {widget}, or {widget, side}).
function decos(state) {
  const out = [];
  // iter() positions the cursor on the first range (goto → next);
  // next() advances and sets value to null when exhausted.
  const cur = state.field(livePreviewState).iter();
  while (cur.value) {
    out.push({ from: cur.from, to: cur.to, spec: cur.value.spec });
    cur.next();
  }
  return out;
}

const lineDecoAt = (ds, from) => ds.find((d) => d.from === d.to && d.from === from && d.spec.class);
const emptyReplace = (ds, from, to) =>
  ds.some((d) => d.from === from && d.to === to && !d.spec.widget);
const widgetReplace = (ds, from) => ds.find((d) => d.from === from && d.spec.widget)?.spec.widget;
const blockAt = (ds, at) => ds.find((d) => d.from === d.to && d.from === at && d.spec.widget);

// The plan renders the selection's lines as source, so a fixture that asserts
// rendered decorations keeps the caret on a line of its own: a trailing empty
// line, which carries no markers.
function rendered(doc, opts = {}) {
  const text = doc.endsWith("\n") ? doc : doc + "\n";
  const st = createEditorState({ ...opts, doc: text });
  return st.update({ selection: { anchor: text.length } }).state;
}

test("the full extension set builds a plain headless EditorState (no DOM)", () => {
  const st = createEditorState({ doc: "" });
  assert.ok(st.field(livePreviewState)); // the plan field is present and populated
});

test("typing '# ' makes the heading line class appear (the marker hides off the caret's line)", () => {
  // Plain text first: no heading class.
  let st = createEditorState({ doc: "Hi\n" });
  assert.equal(lineDecoAt(decos(st), 0), undefined);

  // Type the marker at the top: the plan field must recompute from the new
  // tree, and the caret's line is source.
  st = st.update({ changes: { from: 0, insert: "# " } }).state;
  const line = lineDecoAt(decos(st), 0);
  assert.ok(line, "line decoration with the heading class");
  assert.match(line.spec.class, /tandem-h1/);
  assert.equal(emptyReplace(decos(st), 0, 2), false, "the caret's line shows its source");

  // Move the caret off the line and the marker hides.
  st = st.update({ selection: { anchor: st.doc.length } }).state;
  assert.ok(emptyReplace(decos(st), 0, 2), "the '# ' marker is hidden off the caret's line");
});

test("an unclosed [[ at end of file is not a link", () => {
  const st = createEditorState({ doc: "dangling [[note" });
  const ds = decos(st);
  assert.equal(
    ds.filter((d) => d.spec.widget instanceof WikilinkWidget).length,
    0
  );
});

test("a wikilink resolves against the note-index field (unique basename, case-insensitive)", () => {
  const st = rendered("see [[My Note]]", { notes: ["folder/my note.md"] });
  const w = widgetReplace(decos(st), 4);
  assert.ok(w instanceof WikilinkWidget);
  assert.equal(w.broken, false);
  assert.equal(w.display, "My Note");
  assert.equal(w.target, "My Note");
});

test("duplicate basenames make a bare wikilink broken", () => {
  const st = rendered("[[note]]", { notes: ["a/note.md", "b/note.md"] });
  const w = widgetReplace(decos(st), 0);
  assert.ok(w instanceof WikilinkWidget);
  assert.equal(w.broken, true);
});

test("an empty bikeshed leaves every wikilink broken", () => {
  const st = rendered("[[note]]");
  assert.equal(widgetReplace(decos(st), 0).broken, true);
});

test("setNoteIndex re-resolves links without any document change", () => {
  let st = rendered("[[note]]");
  assert.equal(widgetReplace(decos(st), 0).broken, true);

  st = st.update({ effects: [setNoteIndexEffect.of(["note.md"])] }).state;
  assert.deepEqual(st.field(noteIndexState), ["note.md"]);
  const w = widgetReplace(decos(st), 0);
  assert.ok(w instanceof WikilinkWidget);
  assert.equal(w.broken, false);
});

test("the note path field is held where the plan can see it", () => {
  const st = rendered("x", { notePath: "a/n.md" });
  assert.equal(st.field(notePathState), "a/n.md");
  // Relative image targets resolve against the note's folder in the plan.
  const st2 = rendered("![alt](img/p.png)", { notePath: "a/n.md" });
  const img = blockAt(decos(st2), 0);
  assert.ok(img);
  assert.equal(img.spec.widget.notePath, "a/n.md");
  assert.equal(img.spec.widget.src, "img/p.png");
});

test("block decorations (tables, display math, frontmatter) come from the field", () => {
  const tableSt = rendered("| a | b |\n|---|---|\n| 1 | 2 |");
  const t = blockAt(decos(tableSt), 0);
  assert.ok(t.spec.widget instanceof TableWidget);

  const mathSt = rendered("before\n$$\nx = 1\n$$\nafter");
  const m = blockAt(decos(mathSt), 7);
  assert.ok(m.spec.widget instanceof DisplayMathWidget);
  assert.equal(m.spec.widget.content, "x = 1");

  const fmSt = rendered("---\nt: 1\n---\nbody");
  const f = blockAt(decos(fmSt), 0);
  assert.ok(f.spec.widget instanceof FrontmatterWidget);
  assert.ok(emptyReplace(decos(fmSt), 0, 13), "frontmatter lines hidden from the preview");
});

test("an edit inside a block keeps the block decoration (recompute, not stale)", () => {
  const text = "| a |\n|---|\n| 1 |\n\nafter";
  let st = rendered(text); // the "after" line: the table renders
  assert.ok(blockAt(decos(st), 0)?.spec.widget instanceof TableWidget);
  // Change the cell ("1" at offset 14 → "9"): the table stays, the plan
  // reflects the new content.
  st = st.update({ changes: { from: 14, to: 15, insert: "9" } }).state;
  const t = blockAt(decos(st), 0);
  assert.ok(t.spec.widget instanceof TableWidget);
  assert.deepEqual(t.spec.widget.rows, [["9"]]);
});

// -- reveal: the selection's lines render as source ------------------------------------

test("a selection-only transaction recomputes the plan (document unchanged)", () => {
  let st = createEditorState({ doc: "# One\ntext" });
  // The caret starts on the heading line: its source shows, so no marker hide.
  assert.equal(emptyReplace(decos(st), 0, 2), false);

  st = st.update({ selection: { anchor: 9 } }).state; // the "text" line
  assert.equal(st.doc.toString(), "# One\ntext");
  assert.ok(emptyReplace(decos(st), 0, 2), "the marker hides once the caret leaves the line");
  assert.match(lineDecoAt(decos(st), 0).spec.class, /tandem-h1/);
});

test("a caret inside a table draws its source; leaving restores the widget", () => {
  const text = "| a | b |\n|---|---|\n| 1 | 2 |\n\nafter";
  const tableEnd = text.indexOf("\n\nafter") + 1; // the table's last line break
  let st = createEditorState({ doc: text });
  // The caret starts inside the table: its source lines, no block widget.
  assert.equal(blockAt(decos(st), 0), undefined);
  assert.equal(emptyReplace(decos(st), 0, tableEnd), false);

  st = st.update({ selection: { anchor: text.length } }).state; // the "after" line
  assert.ok(blockAt(decos(st), 0)?.spec.widget instanceof TableWidget);
  assert.ok(emptyReplace(decos(st), 0, tableEnd));
});

test("typing at the start of a revealed heading line keeps the heading", () => {
  let st = createEditorState({ doc: "# Heading\n" });
  assert.equal(emptyReplace(decos(st), 0, 2), false, "the '# ' is source on the caret's line");
  st = st.update({ changes: { from: 2, insert: "Q" } }).state;
  assert.equal(st.doc.toString(), "# QHeading\n");
  assert.match(lineDecoAt(decos(st), 0).spec.class, /tandem-h1/);
});

test("a caret on a wikilink line shows the raw [[…]] instead of the widget", () => {
  let st = createEditorState({ doc: "see [[Note]]\ntail" });
  st = st.update({ selection: { anchor: 6 } }).state; // inside [[Note]]
  assert.equal(widgetReplace(decos(st), 4), undefined);
  assert.equal(emptyReplace(decos(st), 4, 12), false);

  st = st.update({ selection: { anchor: 14 } }).state; // the next line
  assert.ok(widgetReplace(decos(st), 4) instanceof WikilinkWidget);
});
