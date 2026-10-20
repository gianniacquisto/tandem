import { test } from "node:test";
import assert from "node:assert/strict";

import { createEditorState, todoCandidateText } from "../src/main.js";

// The text a "send to todos" action would file: the selection when there is
// one, otherwise the caret's checkbox line, otherwise nothing (the core
// rejects an empty selection). No DOM anywhere in this file.

const state = (doc, anchor, head) =>
  createEditorState({ doc }).update({ selection: { anchor, head } }).state;

test("a selection is the text that would be filed", () => {
  const st = state("Ship the bikeshed\nand the rest", 0, 17);
  assert.equal(todoCandidateText(st), "Ship the bikeshed");
});

test("a selection over several lines is filed whole", () => {
  const st = state("one\ntwo\nthree", 0, 8);
  assert.equal(todoCandidateText(st), "one\ntwo\n");
});

test("an empty selection on a checkbox line files that line", () => {
  const doc = "- [ ] Ship the bikeshed\n- [x] Already done";
  const caret = doc.indexOf("Ship");
  const st = state(doc, caret, caret);
  assert.equal(todoCandidateText(st), "- [ ] Ship the bikeshed");
});

test("an empty selection on a checked line files that line", () => {
  const doc = "- [x] Already done";
  const caret = doc.indexOf("Already");
  const st = state(doc, caret, caret);
  assert.equal(todoCandidateText(st), "- [x] Already done");
});

test("an empty selection on an ordered task item files that line", () => {
  const doc = "1. [ ] Ship the bikeshed";
  const caret = doc.indexOf("Ship");
  const st = state(doc, caret, caret);
  assert.equal(todoCandidateText(st), "1. [ ] Ship the bikeshed");
});

test("an empty selection on a plain line files nothing", () => {
  const doc = "Just a paragraph.";
  const st = state(doc, 5, 5);
  assert.equal(todoCandidateText(st), "");
});

test("an empty selection on a plain list item files nothing", () => {
  const doc = "- a plain bullet";
  const st = state(doc, 4, 4);
  assert.equal(todoCandidateText(st), "");
});

test("a selection wins over the checkbox line under the caret", () => {
  const doc = "- [ ] Ship the bikeshed";
  const st = state(doc, 2, 6); // the marker itself
  assert.equal(todoCandidateText(st), "[ ] ");
});

test("a caret in a plain item nested in a task item files the task line", () => {
  const doc = "- [ ] Ship it\n  - a sub point";
  const caret = doc.indexOf("sub");
  const st = state(doc, caret, caret);
  assert.equal(todoCandidateText(st), "- [ ] Ship it");
});
