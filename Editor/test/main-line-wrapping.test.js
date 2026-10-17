import { test } from "node:test";
import assert from "node:assert/strict";

import { EditorView } from "@codemirror/view";
import { createEditorState, editorExtensions } from "../src/main.js";

// The pane never scrolls sideways: long lines soft-wrap. The wrapping is
// CodeMirror's own EditorView.lineWrapping extension in the editor's
// extension set (theme.js' overflowWrap only handles a single unbreakable
// token, which wrapping alone would leave overflowing).

test("editorExtensions includes line wrapping", () => {
  assert.equal(editorExtensions().includes(EditorView.lineWrapping), true);
});

test("a mounted editor's state turns wrapping on", () => {
  const st = createEditorState({ doc: "x".repeat(5000) });
  // The facet value is lineWrapping's own theme spec ({class:
  // "cm-lineWrapping"}); it is absent (undefined) when wrapping is off.
  assert.deepEqual(st.facet(EditorView.lineWrapping), {
    class: "cm-lineWrapping",
  });
});
