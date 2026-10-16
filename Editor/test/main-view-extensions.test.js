import { test, afterEach } from "node:test";
import assert from "node:assert/strict";

import { EditorView } from "@codemirror/view";
import {
  createEditorState,
  editorViewExtensions,
  setDocumentSpec,
  insertTextSpec,
} from "../src/main.js";

// The view-level extensions (the dirty update listener, the link-click
// handler) must live in the STATE, not the EditorView config's
// `extensions`: with a supplied `state`, the EditorView constructor
// ignores that config array and reads the `updateListener` facet and
// the plugin facet from `this.state` — a listener handed in the config
// silently never fires. That was the first-human-GUI-pass bug: nothing
// ever autosaved (and markdown-link clicks died with it). These tests
// pin the contract the view relies on.

let posted;
afterEach(() => {
  posted = undefined;
  delete globalThis.window;
});
function stubBridge() {
  posted = [];
  globalThis.window = {
    webkit: {
      messageHandlers: {
        tandem: {
          postMessage: (s) => posted.push(JSON.parse(s)),
        },
      },
    },
  };
}

// The state the view is built from: the same call mountEditor makes.
const viewState = (opts = {}) =>
  createEditorState({ viewExtensions: editorViewExtensions(), ...opts });

// -- The state carries the extensions the view reads from it -------------------------

test("the state carries the update listener (the view reads it from this.state)", () => {
  const listeners = viewState().facet(EditorView.updateListener);
  assert.equal(listeners.length, 1);
  assert.equal(typeof listeners[0], "function");
});

// -- The click handler (the extension the view installs as a plugin) -------------------

// Click targets (the same shapes the bridge's linkOpenTargetFrom tests
// use): closest(sel) finds the marked element, and the mark carries its
// payload attribute — what the widgets' toDOM (data-target /
// data-marker-from) and the plan's spans (data-href) produce.
const markEl = (attr, value) => ({
  getAttribute: (name) => (name === attr ? value : null),
});
const targetEl = (marks) => ({
  closest: (sel) => (marks && sel in marks ? marks[sel] : null),
});
const clickHandler = () => editorViewExtensions()[0].domEventHandlers.click;
const mousedownHandler = () => editorViewExtensions()[0].domEventHandlers.mousedown;

test("a click on a .tandem-link mark posts link-opened and is handled", () => {
  stubBridge();
  const handled = clickHandler()(
    { target: targetEl({ ".tandem-link": markEl("data-href", "Notes/Other.md") }) }
  );
  assert.equal(handled, true);
  assert.deepEqual(posted, [{ type: "link-opened", target: "Notes/Other.md" }]);
});

test("a click on a wikilink widget posts its raw data-target", () => {
  stubBridge();
  const handled = clickHandler()(
    { target: targetEl({ ".tandem-wikilink": markEl("data-target", "Ideas") }) }
  );
  assert.equal(handled, true);
  assert.deepEqual(posted, [{ type: "link-opened", target: "Ideas" }]);
});

test("a click on a checkbox widget dispatches the toggle transaction", () => {
  stubBridge();
  const st = viewState({ doc: "- [ ] task" });
  let spec;
  const view = { state: st, dispatch: (s) => (spec = s) };
  const handled = clickHandler()(
    { target: targetEl({ ".tandem-checkbox": markEl("data-marker-from", "2") }) },
    view
  );
  assert.equal(handled, true);
  assert.equal(spec.changes.from, 3);
  assert.equal(spec.changes.insert, "x");
  assert.deepEqual(posted, [{ type: "checkbox-toggled", line: 1, checked: true }]);
});

test("a click off any link or checkbox is passed through and posts nothing", () => {
  stubBridge();
  assert.equal(clickHandler()({ target: targetEl(null) }), false);
  assert.deepEqual(posted, []);
});

test("a mousedown on a checkbox widget is handled (focus is kept)", () => {
  assert.equal(
    mousedownHandler()(
      { target: targetEl({ ".tandem-checkbox": markEl("data-marker-from", "2") }) }
    ),
    true
  );
});

test("a mousedown off a checkbox is passed through", () => {
  assert.equal(mousedownHandler()({ target: targetEl(null) }), false);
  assert.equal(
    mousedownHandler()(
      { target: targetEl({ ".tandem-link": markEl("data-href", "x") }) }
    ),
    false
  );
});

// -- The listener, driven with real transactions --------------------------------------

test("a document change posts dirty with the full current text", () => {
  stubBridge();
  let st = viewState({ doc: "ab cd" });
  st = st.update({ selection: { anchor: 3 } }).state; // caret before "cd"
  const tr = st.update(insertTextSpec(st, "XY"));
  const [listener] = tr.state.facet(EditorView.updateListener);
  listener({ transactions: [tr], state: tr.state });
  assert.deepEqual(posted, [{ type: "dirty", text: "ab XYcd" }]);
});

test("a raw user-input change posts dirty (the typing path)", () => {
  stubBridge();
  const st = viewState({ doc: "hello" });
  const tr = st.update({ changes: { from: 0, insert: "X" } });
  const [listener] = tr.state.facet(EditorView.updateListener);
  listener({ transactions: [tr], state: tr.state });
  assert.deepEqual(posted, [{ type: "dirty", text: "Xhello" }]);
});

test("the bridge's own setDocument posts nothing (it replaces, it does not dirty)", () => {
  stubBridge();
  const st = viewState({ doc: "old" });
  const tr = st.update(setDocumentSpec(st, "# Fresh", "n.md"));
  const [listener] = tr.state.facet(EditorView.updateListener);
  listener({ transactions: [tr], state: tr.state });
  assert.deepEqual(posted, []);
});

test("a selection-only update posts nothing", () => {
  stubBridge();
  const st = viewState({ doc: "one\ntwo" });
  const tr = st.update({ selection: { anchor: 4 } });
  const [listener] = tr.state.facet(EditorView.updateListener);
  listener({ transactions: [tr], state: tr.state });
  assert.deepEqual(posted, []);
});
