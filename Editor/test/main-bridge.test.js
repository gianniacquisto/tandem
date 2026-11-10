import { test, afterEach } from "node:test";
import assert from "node:assert/strict";

import { EditorState } from "@codemirror/state";
import {
  dirtyTextForUpdate,
  setDocumentAnnotation,
  setDocumentSpec,
  linkOpenTargetFrom,
  checkboxToggleFrom,
} from "../src/main.js";
import { postLoaded, postDirty, postLinkOpened, postCheckboxToggled } from "../src/bridge.js";
import {
  toggleTask,
  WikilinkWidget,
  CheckboxWidget,
  BulletWidget,
} from "../src/widgets.js";

// Bridge contract (notes/bridge-contract.md), headless: `send` reads
// window.webkit.messageHandlers.tandem at call time, so a global stub
// captures every JS → Swift message. No real DOM anywhere in this file.

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

// -- JS → Swift messages -------------------------------------------------------------

test("loaded is posted when the page is ready", () => {
  stubBridge();
  postLoaded();
  assert.deepEqual(posted, [{ type: "loaded" }]);
});

test("dirty carries the full raw text", () => {
  stubBridge();
  postDirty("# heading\nbody");
  assert.deepEqual(posted, [{ type: "dirty", text: "# heading\nbody" }]);
});

test("link-opened carries the raw target", () => {
  stubBridge();
  postLinkOpened("Notes/Other.md");
  assert.deepEqual(posted, [{ type: "link-opened", target: "Notes/Other.md" }]);
});

test("checkbox-toggled carries the 1-based line and the new state", () => {
  stubBridge();
  postCheckboxToggled(3, true);
  assert.deepEqual(posted, [{ type: "checkbox-toggled", line: 3, checked: true }]);
});

// -- dirty decision: what the update listener emits ----------------------------------

test("a document change emits the full current text", () => {
  const st = EditorState.create({ doc: "hi" });
  const tr = st.update({ changes: { from: 0, insert: "o" } });
  assert.equal(dirtyTextForUpdate([tr], tr.state), "ohi");
});

test("a selection-only update emits nothing", () => {
  const st = EditorState.create({ doc: "hello" });
  const tr = st.update({ selection: { anchor: 2 } });
  assert.equal(dirtyTextForUpdate([tr], tr.state), null);
});

test("the bridge's own setDocument emits nothing (it replaces, it does not dirty)", () => {
  const st = EditorState.create({ doc: "old" });
  const tr = st.update(setDocumentSpec(st, "fresh doc", "a/n.md"));
  assert.equal(tr.state.doc.toString(), "fresh doc");
  assert.equal(dirtyTextForUpdate([tr], tr.state), null);
});

test("an annotated change in a batch does not mask a real one", () => {
  const st = EditorState.create({ doc: "a" });
  const replaced = st.update(setDocumentSpec(st, "bb", "a/n.md"));
  const typed = replaced.state.update({ changes: { from: 0, insert: "c" } });
  // The batch: setDocument (annotated) + typing → the full text is emitted.
  assert.equal(
    dirtyTextForUpdate([replaced, typed], typed.state),
    "cbb"
  );
  // And the annotated transaction alone would have emitted nothing.
  assert.equal(dirtyTextForUpdate([replaced], replaced.state), null);
});

// -- Checkbox toggle: the transaction + payload ---------------------------------------

// toggleTask takes a view-like {state, dispatch}: the fake captures the
// dispatch spec; the state is a real (headless) EditorState so the flip
// is read from the buffer, the source of truth.
function toggle(doc, markerFrom) {
  const st = EditorState.create({ doc });
  let dispatched;
  const view = {
    state: st,
    dispatch: (spec) => {
      dispatched = spec;
    },
  };
  toggleTask(view, markerFrom);
  return { dispatched, posted: [...posted] };
}

test("clicking an unchecked task flips ' ' to 'x' and posts the payload", () => {
  stubBridge();
  // "- [ ] task": TaskMarker at 2, the state char (" ") at 3.
  const { dispatched, posted: msgs } = toggle("- [ ] task", 2);
  assert.ok(dispatched, "the toggle dispatches a transaction");
  assert.equal(dispatched.changes.from, 3);
  assert.equal(dispatched.changes.to, 4);
  assert.equal(dispatched.changes.insert, "x");
  assert.equal(dispatched.userEvent, "tandem.toggleTask");
  assert.deepEqual(msgs, [{ type: "checkbox-toggled", line: 1, checked: true }]);
});

test("clicking a checked task flips 'x' back to ' '", () => {
  stubBridge();
  // Second line of a task list: "[x]" marker at 13, state char at 14.
  const { dispatched, posted: msgs } = toggle("- [ ] task\n- [x] done", 13);
  assert.ok(dispatched);
  assert.equal(dispatched.changes.from, 14);
  assert.equal(dispatched.changes.insert, " ");
  assert.deepEqual(msgs, [{ type: "checkbox-toggled", line: 2, checked: false }]);
});

test("a click that is not a task marker flips nothing and posts nothing", () => {
  stubBridge();
  const { dispatched, posted: msgs } = toggle("plain text", 0);
  assert.equal(dispatched, undefined);
  assert.deepEqual(msgs, []);
});

// -- Link click decision (the webview never navigates) --------------------------------

// The DOM shapes the decision walks: closest(sel) finds the marked
// element (if any) and the mark carries its payload attribute — exactly
// what the widgets' toDOM (data-target / data-marker-from) and the
// plan's spans (data-href) produce.
const markEl = (attr, value) => ({
  getAttribute: (name) => (name === attr ? value : null),
});
const targetEl = (marks) => ({
  closest: (sel) => (marks && sel in marks ? marks[sel] : null),
});

test("a click inside a .tandem-link mark returns its data-href", () => {
  assert.equal(
    linkOpenTargetFrom(
      targetEl({ ".tandem-link": markEl("data-href", "Notes/Other.md") })
    ),
    "Notes/Other.md"
  );
});

test("a click inside a wikilink widget returns its raw data-target", () => {
  assert.equal(
    linkOpenTargetFrom(
      targetEl({ ".tandem-wikilink": markEl("data-target", "Ideas") })
    ),
    "Ideas"
  );
});

test("a wikilink target with an alias is posted as written (Swift strips it)", () => {
  assert.equal(
    linkOpenTargetFrom(
      targetEl({ ".tandem-wikilink": markEl("data-target", "Ideas|the ideas note") })
    ),
    "Ideas|the ideas note"
  );
});

test("a click that is not on a link returns null", () => {
  assert.equal(linkOpenTargetFrom(targetEl(null)), null);
});

test("a mark without a usable target returns null", () => {
  assert.equal(
    linkOpenTargetFrom(targetEl({ ".tandem-wikilink": markEl("data-target", "") })),
    null
  );
  assert.equal(
    linkOpenTargetFrom(targetEl({ ".tandem-link": markEl("data-href", "") })),
    null
  );
  assert.equal(
    linkOpenTargetFrom(targetEl({ ".tandem-link": markEl("data-href", null) })),
    null
  );
});

test("a non-element target returns null", () => {
  assert.equal(linkOpenTargetFrom(null), null);
  assert.equal(linkOpenTargetFrom({}), null);
});

// -- Interactive widgets must not ignore editor events -----------------------------------

// The default WidgetType.ignoreEvent() === true makes the view drop every
// bubbling event from the widget (eventBelongsToEditor) before any
// domEventHandlers runs — the second GUI-pass symptom (dead wikilink and
// checkbox clicks). The interactive widgets override it to false; the
// inert ones keep the default (all events ignored by the editor).
test("wikilink and checkbox widgets pass events through to the editor", () => {
  assert.equal(new WikilinkWidget("Ideas", "Ideas", false).ignoreEvent(), false);
  assert.equal(new CheckboxWidget(false, 2).ignoreEvent(), false);
  assert.equal(new BulletWidget(undefined).ignoreEvent(), true);
});

// -- Checkbox click decision ------------------------------------------------------------

test("a click on a checkbox widget returns its data-marker-from", () => {
  assert.equal(
    checkboxToggleFrom(
      targetEl({ ".tandem-checkbox": markEl("data-marker-from", "13") })
    ),
    13
  );
});

test("a checkbox at buffer position 0 is valid (falsy-zero trap)", () => {
  assert.equal(
    checkboxToggleFrom(
      targetEl({ ".tandem-checkbox": markEl("data-marker-from", "0") })
    ),
    0
  );
});

test("a click off a checkbox returns null", () => {
  assert.equal(checkboxToggleFrom(targetEl(null)), null);
  assert.equal(
    checkboxToggleFrom(targetEl({ ".tandem-link": markEl("data-href", "x") })),
    null
  );
  assert.equal(checkboxToggleFrom(null), null);
  assert.equal(checkboxToggleFrom({}), null);
});

test("a checkbox without a usable marker position returns null", () => {
  for (const bad of [null, "", " 2", "1.5", "-1", "abc", "0x2"]) {
    assert.equal(
      checkboxToggleFrom(
        targetEl({ ".tandem-checkbox": markEl("data-marker-from", bad) })
      ),
      null,
      JSON.stringify(bad)
    );
  }
});
