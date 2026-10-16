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

// A .tandem-link mark, and a click target that walks up to it (the
// same shapes the bridge's linkOpenTargetFrom tests use).
const markEl = (href) => ({
  getAttribute: (name) => (name === "data-href" ? href : null),
});
const targetEl = (mark) => ({
  closest: (sel) => (sel === ".tandem-link" ? mark : null),
});
const clickHandler = () => editorViewExtensions()[0].domEventHandlers.click;

test("a click on a .tandem-link mark posts link-opened and is handled", () => {
  stubBridge();
  const handled = clickHandler()({ target: targetEl(markEl("Notes/Other.md")) });
  assert.equal(handled, true);
  assert.deepEqual(posted, [{ type: "link-opened", target: "Notes/Other.md" }]);
});

test("a click off any link is passed through and posts nothing", () => {
  stubBridge();
  assert.equal(clickHandler()({ target: targetEl(null) }), false);
  assert.deepEqual(posted, []);
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
