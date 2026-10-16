import { test } from "node:test";
import assert from "node:assert/strict";

import {
  createEditorState,
  livePreviewState,
  vaultNotesState,
  notePathState,
  setVaultIndexEffect,
  setNotePathEffect,
} from "../src/main.js";
import { WikilinkWidget, TableWidget, DisplayMathWidget, FrontmatterWidget } from "../src/widgets.js";

// The live-preview plan field contract: a StateField over a plain EditorState
// that recomputes the blocks.js plan from syntaxTree(state) + the vault
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

test("the full extension set builds a plain headless EditorState (no DOM)", () => {
  const st = createEditorState({ doc: "" });
  assert.ok(st.field(livePreviewState)); // the plan field is present and populated
});

test("typing '# ' makes the heading line class appear and hides the marker", () => {
  // Plain text first: no heading class.
  let st = createEditorState({ doc: "Hi" });
  assert.equal(lineDecoAt(decos(st), 0), undefined);

  // Type the marker: the plan field must recompute from the new tree.
  // state.update() returns the Transaction; .state is the new state.
  st = st.update({ changes: { from: 0, insert: "# " } }).state;
  const ds = decos(st);
  const line = lineDecoAt(ds, 0);
  assert.ok(line, "line decoration with the heading class");
  assert.match(line.spec.class, /tandem-h1/);
  assert.ok(emptyReplace(ds, 0, 2), "the '# ' marker is hidden");
});

test("an unclosed [[ at end of file is not a link", () => {
  const st = createEditorState({ doc: "dangling [[note" });
  const ds = decos(st);
  assert.equal(
    ds.filter((d) => d.spec.widget instanceof WikilinkWidget).length,
    0
  );
});

test("a wikilink resolves against the vault field (unique basename, case-insensitive)", () => {
  const st = createEditorState({
    doc: "see [[My Note]]",
    vaultNotes: ["folder/my note.md"],
  });
  const w = widgetReplace(decos(st), 4);
  assert.ok(w instanceof WikilinkWidget);
  assert.equal(w.broken, false);
  assert.equal(w.display, "My Note");
  assert.equal(w.target, "My Note");
});

test("duplicate basenames make a bare wikilink broken", () => {
  const st = createEditorState({
    doc: "[[note]]",
    vaultNotes: ["a/note.md", "b/note.md"],
  });
  const w = widgetReplace(decos(st), 0);
  assert.ok(w instanceof WikilinkWidget);
  assert.equal(w.broken, true);
});

test("an empty vault leaves every wikilink broken", () => {
  const st = createEditorState({ doc: "[[note]]" });
  assert.equal(widgetReplace(decos(st), 0).broken, true);
});

test("setVaultIndex re-resolves links without any document change", () => {
  let st = createEditorState({ doc: "[[note]]" });
  assert.equal(widgetReplace(decos(st), 0).broken, true);

  st = st.update({ effects: [setVaultIndexEffect.of(["note.md"])] }).state;
  assert.deepEqual(st.field(vaultNotesState), ["note.md"]);
  const w = widgetReplace(decos(st), 0);
  assert.ok(w instanceof WikilinkWidget);
  assert.equal(w.broken, false);
});

test("the note path field is held where the plan can see it", () => {
  const st = createEditorState({ doc: "x", notePath: "a/n.md" });
  assert.equal(st.field(notePathState), "a/n.md");
  // Relative image targets resolve against the note's folder in the plan.
  const st2 = createEditorState({
    doc: "![alt](img/p.png)",
    notePath: "a/n.md",
  });
  const img = blockAt(decos(st2), 0);
  assert.ok(img);
  assert.equal(img.spec.widget.notePath, "a/n.md");
  assert.equal(img.spec.widget.src, "img/p.png");
});

test("block decorations (tables, display math, frontmatter) come from the field", () => {
  const tableSt = createEditorState({ doc: "| a | b |\n|---|---|\n| 1 | 2 |" });
  const t = blockAt(decos(tableSt), 0);
  assert.ok(t.spec.widget instanceof TableWidget);

  const mathSt = createEditorState({ doc: "before\n$$\nx = 1\n$$\nafter" });
  const m = blockAt(decos(mathSt), 7);
  assert.ok(m.spec.widget instanceof DisplayMathWidget);
  assert.equal(m.spec.widget.content, "x = 1");

  const fmSt = createEditorState({ doc: "---\nt: 1\n---\nbody" });
  const f = blockAt(decos(fmSt), 0);
  assert.ok(f.spec.widget instanceof FrontmatterWidget);
  assert.ok(emptyReplace(decos(fmSt), 0, 13), "frontmatter lines hidden from the preview");
});

test("an edit inside a block keeps the block decoration (recompute, not stale)", () => {
  let st = createEditorState({ doc: "| a |\n|---|\n| 1 |" });
  assert.ok(blockAt(decos(st), 0)?.spec.widget instanceof TableWidget);
  // Change the cell ("1" at offset 14 → "9"): the table stays, the plan
  // reflects the new content.
  st = st.update({ changes: { from: 14, to: 15, insert: "9" } }).state;
  const t = blockAt(decos(st), 0);
  assert.ok(t.spec.widget instanceof TableWidget);
  assert.deepEqual(t.spec.widget.rows, [["9"]]);
});
