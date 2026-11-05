import { test } from "node:test";
import assert from "node:assert/strict";

import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import {
  createEditorState,
  linkOpenTargetFrom,
  checkboxToggleFrom,
  setModeSpec,
  isPreviewMode,
  toolbarVisibleForMode,
} from "../src/main.js";
import { toggleTask } from "../src/widgets.js";

// Preview is the same document read-only: the plan/widgets stay live,
// typing is blocked, and the toolbar hides. No second renderer.

// -- readOnly + editable -----------------------------------------------------

test("a preview state is readOnly and not editable, an edit state is the reverse", () => {
  const preview = createEditorState({ doc: "hi", mode: "preview" });
  assert.equal(preview.readOnly, true);
  assert.equal(preview.facet(EditorView.editable), false);

  const edit = createEditorState({ doc: "hi", mode: "edit" });
  assert.equal(edit.readOnly, false);
  assert.equal(edit.facet(EditorView.editable), true);
});

test("the default state is editable (edit mode)", () => {
  const st = createEditorState({ doc: "hi" });
  assert.equal(st.readOnly, false);
  assert.equal(st.facet(EditorView.editable), true);
  assert.equal(isPreviewMode(st), false);
});

test("setMode toggles readOnly without touching the document", () => {
  let st = createEditorState({ doc: "# Note", mode: "edit" });
  st = st.update(setModeSpec("preview")).state;
  assert.equal(st.doc.toString(), "# Note");
  assert.equal(st.readOnly, true);
  assert.equal(st.facet(EditorView.editable), false);
  assert.equal(isPreviewMode(st), true);

  st = st.update(setModeSpec("edit")).state;
  assert.equal(st.readOnly, false);
  assert.equal(st.facet(EditorView.editable), true);
  assert.equal(isPreviewMode(st), false);
});

// -- links and checkboxes stay live in preview --------------------------------

const markEl = (attr, value) => ({
  getAttribute: (name) => (name === attr ? value : null),
});
const targetEl = (marks) => ({
  closest: (sel) => (marks && sel in marks ? marks[sel] : null),
});

test("link targets and checkbox markers still resolve in preview", () => {
  const preview = createEditorState({ doc: "see [[Note]]", mode: "preview" });
  assert.equal(preview.readOnly, true);
  // The decisions are pure and mode-independent: the preview keeps them live.
  assert.equal(
    linkOpenTargetFrom(targetEl({ ".tandem-wikilink": markEl("data-target", "Note") })),
    "Note"
  );
  assert.equal(
    linkOpenTargetFrom(targetEl({ ".tandem-link": markEl("data-href", "a/b.md") })),
    "a/b.md"
  );
  assert.equal(
    checkboxToggleFrom(targetEl({ ".tandem-checkbox": markEl("data-marker-from", "2") })),
    2
  );
});

test("the checkbox toggle transaction still dispatches in preview", () => {
  globalThis.window = {
    webkit: { messageHandlers: { tandem: { postMessage: () => {} } } },
  };
  try {
    const st = createEditorState({ doc: "- [ ] task", mode: "preview" });
    let dispatched;
    const view = {
      state: st,
      dispatch: (spec) => {
        dispatched = spec;
      },
    };
    toggleTask(view, 2);
    assert.ok(dispatched, "the toggle dispatches even when readOnly");
    assert.equal(dispatched.changes.from, 3);
  } finally {
    delete globalThis.window;
  }
});

// -- typing is blocked in preview ----------------------------------------------

test("a preview state reports readOnly so input handlers ignore typing", () => {
  const preview = createEditorState({ doc: "hi", mode: "preview" });
  const edit = createEditorState({ doc: "hi", mode: "edit" });
  // The view's input handlers consult exactly this flag to drop keystrokes,
  // paste and drop; editable removes the caret and contenteditable.
  assert.equal(preview.readOnly, true);
  assert.equal(preview.facet(EditorView.editable), false);
  assert.equal(edit.readOnly, false);
});

// -- toolbar -------------------------------------------------------------------

test("the toolbar is hidden in preview and shown in edit", () => {
  assert.equal(toolbarVisibleForMode("preview"), false);
  assert.equal(toolbarVisibleForMode("edit"), true);
});
