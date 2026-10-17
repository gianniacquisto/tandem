import { test } from "node:test";
import assert from "node:assert/strict";

import {
  createEditorState,
  dirtyTextForUpdate,
  wrapSelectionSpec,
  toggleLinePrefixSpec,
  cycleHeadingSpec,
  fencedBlockSpec,
  specFor,
  WRAP_PLACEHOLDER,
  DEFAULT_EDITOR_FONT_SIZE,
  MIN_EDITOR_FONT_SIZE,
  MAX_EDITOR_FONT_SIZE,
  clampFontSize,
  stepFontSize,
} from "../src/main.js";

// The formatting actions (the toolbar's edits), headless: each is a pure
// TransactionSpec builder, so applying it to a plain EditorState exercises
// the whole effect — the toolbar only dispatches the same spec from the
// window global. No DOM anywhere in this file.

// Apply a selection, then a spec: the same order the view does.
function run(doc, selection, spec) {
  let st = createEditorState({ doc });
  if (selection) st = st.update({ selection }).state;
  return st.update(spec(st)).state;
}

const docText = (st) => st.doc.toString();
const selected = (st) => st.doc.sliceString(st.selection.main.from, st.selection.main.to);

// -- wrapSelectionSpec ----------------------------------------------------------------

test("wrap with an empty selection inserts the placeholder and selects it", () => {
  const st = run("hello", { anchor: 5 }, (s) => wrapSelectionSpec(s, "**"));
  assert.equal(docText(st), "hello**" + WRAP_PLACEHOLDER + "**");
  assert.equal(selected(st), WRAP_PLACEHOLDER);
});

test("wrap with a selection brackets it and keeps it selected", () => {
  const st = run("ab cd", { anchor: 0, head: 2 }, (s) => wrapSelectionSpec(s, "**"));
  assert.equal(docText(st), "**ab** cd");
  assert.equal(selected(st), "**ab**");
});

test("wrap works mid-line, after text", () => {
  const st = run("ab cd", { anchor: 3, head: 5 }, (s) => wrapSelectionSpec(s, "*"));
  assert.equal(docText(st), "ab *cd*");
});

test("wrap takes distinct before/after (the link case)", () => {
  const st = run("hi", { anchor: 0, head: 2 }, (s) => wrapSelectionSpec(s, "[", "](url)"));
  assert.equal(docText(st), "[hi](url)");
});

test("a formatting wrap dirties (it is an ordinary document change)", () => {
  const st0 = createEditorState({ doc: "ab" });
  const st = st0.update({ selection: { anchor: 0, head: 2 } }).state;
  const tr = st.update(wrapSelectionSpec(st, "**"));
  assert.equal(dirtyTextForUpdate([tr], tr.state), "**ab**");
});

// -- toggleLinePrefixSpec -------------------------------------------------------------

test("toggle adds the prefix to every touched line", () => {
  const st = run("one\ntwo", { anchor: 0, head: 7 }, (s) => toggleLinePrefixSpec(s, "> "));
  assert.equal(docText(st), "> one\n> two");
});

test("toggle removes the prefix when every touched line already has it", () => {
  const st = run("> one\n> two", { anchor: 0, head: 11 }, (s) =>
    toggleLinePrefixSpec(s, "> ")
  );
  assert.equal(docText(st), "one\ntwo");
});

test("toggle adds only to the lines that lack the prefix", () => {
  const st = run("> one\ntwo", { anchor: 0, head: 9 }, (s) => toggleLinePrefixSpec(s, "> "));
  assert.equal(docText(st), "> one\n> two");
});

test("toggle ignores leading indentation when deciding", () => {
  const st = run("  > one\ntwo", { anchor: 0, head: 11 }, (s) =>
    toggleLinePrefixSpec(s, "> ")
  );
  // Not all lines have it (the bare "two"), so it is added there; the
  // indented line is left alone.
  assert.equal(docText(st), "  > one\n> two");
});

test("toggle inserts after indentation, preserving it", () => {
  const st = run("  one", { anchor: 0, head: 5 }, (s) => toggleLinePrefixSpec(s, "- "));
  assert.equal(docText(st), "  - one");
});

test("toggle removes after indentation, preserving it", () => {
  const st = run("  - one", { anchor: 0, head: 7 }, (s) => toggleLinePrefixSpec(s, "- "));
  assert.equal(docText(st), "  one");
});

test("toggling twice is a no-op", () => {
  let st = createEditorState({ doc: "a\nb" });
  st = st.update({ selection: { anchor: 0, head: 3 } }).state;
  st = st.update(toggleLinePrefixSpec(st, "- ")).state;
  st = st.update(toggleLinePrefixSpec(st, "- ")).state;
  assert.equal(docText(st), "a\nb");
});

test("a selection ending at a line boundary does not touch the next line", () => {
  // "a\nb" with the selection covering only "a\n" (anchor 0, head 2).
  const st = run("a\nb", { anchor: 0, head: 2 }, (s) => toggleLinePrefixSpec(s, "- "));
  assert.equal(docText(st), "- a\nb");
});

test("the task-list prefix is inserted as syntax (checkboxes stay interactive)", () => {
  const st = run("todo", { anchor: 0, head: 4 }, (s) => specFor("task-list", s));
  assert.equal(docText(st), "- [ ] todo");
});

// -- cycleHeadingSpec -----------------------------------------------------------------

test("heading cycles body → H1 → H2 → H3 → body", () => {
  const levels = ["Title", "# Title", "## Title", "### Title"];
  for (let i = 0; i < levels.length; i++) {
    const st = run(levels[i], { anchor: 0, head: levels[i].length }, cycleHeadingSpec);
    assert.equal(docText(st), levels[(i + 1) % levels.length], levels[i]);
  }
});

test("a deep heading (H6) cycles back to body", () => {
  const st = run("###### Title", { anchor: 0, head: 8 }, cycleHeadingSpec);
  assert.equal(docText(st), "Title");
});

test("heading cycles every touched line", () => {
  const st = run("A\nB", { anchor: 0, head: 3 }, cycleHeadingSpec);
  assert.equal(docText(st), "# A\n# B");
});

test("heading preserves indentation", () => {
  const st = run("  Title", { anchor: 0, head: 7 }, cycleHeadingSpec);
  assert.equal(docText(st), "  # Title");
});

test("an indented heading keeps its indent through the cycle", () => {
  const st = run("  ## Title", { anchor: 0, head: 10 }, cycleHeadingSpec);
  assert.equal(docText(st), "  ### Title");
});

// -- fencedBlockSpec ------------------------------------------------------------------

test("a code block fences the touched lines", () => {
  const st = run("let x = 1", { anchor: 0, head: 9 }, (s) => specFor("code-block", s));
  assert.equal(docText(st), "```\nlet x = 1\n```");
  assert.equal(selected(st), "let x = 1");
});

test("a code block over multiple lines fences them as one block", () => {
  const st = run("a\nb", { anchor: 0, head: 3 }, fencedBlockSpec);
  assert.equal(docText(st), "```\na\nb\n```");
  assert.equal(selected(st), "a\nb");
});

test("a code block on an empty selection leaves the caret inside", () => {
  const st = run("", { anchor: 0 }, fencedBlockSpec);
  assert.equal(docText(st), "```\n\n```");
  assert.equal(st.selection.main.from, 4);
  assert.equal(st.selection.main.empty, true);
});

test("a second code block removes the fence (round-trip)", () => {
  const st = run("a\nb", { anchor: 0, head: 3 }, fencedBlockSpec);
  const back = st.update(fencedBlockSpec(st)).state;
  assert.equal(docText(back), "a\nb");
  assert.equal(selected(back), "a\nb");
});

// -- specFor --------------------------------------------------------------------------

test("specFor maps each toolbar kind to a transaction", () => {
  const cases = {
    bold: ["ab", "**ab**"],
    italic: ["ab", "*ab*"],
    strikethrough: ["ab", "~~ab~~"],
    "inline-code": ["ab", "`ab`"],
    "bullet-list": ["ab", "- ab"],
    "numbered-list": ["ab", "1. ab"],
    quote: ["ab", "> ab"],
    "code-block": ["ab", "```\nab\n```"],
  };
  for (const [kind, [before, after]] of Object.entries(cases)) {
    const st = run(before, { anchor: 0, head: before.length }, (s) => specFor(kind, s));
    assert.equal(docText(st), after, kind);
  }
});

test("specFor rejects an unknown kind", () => {
  const st = createEditorState({ doc: "" });
  assert.throws(() => specFor("nope", st), RangeError);
});

// -- Font-size helpers ----------------------------------------------------------------

test("the default snaps onto integers on the first step", () => {
  assert.equal(stepFontSize(DEFAULT_EDITOR_FONT_SIZE, 1), 14);
  assert.equal(stepFontSize(DEFAULT_EDITOR_FONT_SIZE, -1), 13);
  assert.equal(stepFontSize(14, 1), 15);
  assert.equal(stepFontSize(14, -1), 13);
});

test("the step clamps to the sane range", () => {
  assert.equal(stepFontSize(MIN_EDITOR_FONT_SIZE, -1), MIN_EDITOR_FONT_SIZE);
  assert.equal(stepFontSize(MAX_EDITOR_FONT_SIZE, 1), MAX_EDITOR_FONT_SIZE);
});

test("clampFontSize holds the range and falls back on nonsense", () => {
  assert.equal(clampFontSize(9), MIN_EDITOR_FONT_SIZE);
  assert.equal(clampFontSize(99), MAX_EDITOR_FONT_SIZE);
  assert.equal(clampFontSize("16"), 16);
  assert.equal(clampFontSize("nope"), DEFAULT_EDITOR_FONT_SIZE);
});
