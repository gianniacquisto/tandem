import { test } from "node:test";
import assert from "node:assert/strict";

import { WikilinkWidget, toggleTask } from "../src/widgets.js";
import { createEditorState } from "../src/main.js";

// TAN-305 repro: the alias an editor types after the pipe must paint.
// Same target, different display text is a different widget: eq() must be
// false so the DOM node is rebuilt with the new alias.
test("a wikilink with a different alias is not equal (the alias repaints)", () => {
  const first = new WikilinkWidget("note", "first", false);
  const second = new WikilinkWidget("note", "second", false);
  assert.equal(first.eq(second), false);
});

test("a wikilink with the same target and display stays equal", () => {
  const first = new WikilinkWidget("note", "same", false);
  const second = new WikilinkWidget("note", "same", false);
  assert.equal(first.eq(second), true);
});

// TAN-305 repro: an uppercase [X] is checked (blocks.js treats x/X the
// same), so clicking its checkbox must uncheck it. The widget click
// dispatches the one-character transaction; the buffer is the source of
// truth, so the check reads the buffer, not the widget.
test("clicking an uppercase [X] checkbox unchecks it", () => {
  globalThis.window = {
    webkit: { messageHandlers: { tandem: { postMessage: () => {} } } },
  };
  try {
    const st = createEditorState({ doc: "- [X] done" });
    let spec;
    const view = { state: st, dispatch: (s) => (spec = s) };
    toggleTask(view, 2);
    assert.ok(spec, "a toggle transaction is dispatched");
    const next = st.update(spec).state;
    assert.equal(next.doc.toString(), "- [ ] done");
  } finally {
    delete globalThis.window;
  }
});
