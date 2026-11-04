import { test } from "node:test";
import assert from "node:assert/strict";

import { scanHighlights } from "../src/pure/highlight.js";

// Spec (TAND-10): GitHub-style ==highlight== marks, Obsidian trigger rules —
// one line, non-empty content with non-space edges, exactly-two `==`
// delimiters (a `===` run is literal). Cross-cutting: nothing is processed
// inside code fences, inline code spans, a leading frontmatter block, or a
// caller-supplied mask range.

const ranges = (text, masks) => scanHighlights(text, masks).map((m) => [m.from, m.to]);

test("basic mark: delimiters covered, content between them", () => {
  const r = scanHighlights("a ==hi== b");
  assert.equal(r.length, 1);
  assert.equal(r[0].content, "hi");
  assert.deepEqual(ranges("a ==hi== b"), [[2, 8]]);
});

test("two marks on one line", () => {
  assert.deepEqual(ranges("==a== and ==b=="), [
    [0, 5],
    [10, 15],
  ]);
});

test("space right after the opener is not a mark", () => {
  assert.equal(scanHighlights("a == hi== b").length, 0);
});

test("space right before the closer is not a mark", () => {
  assert.equal(scanHighlights("a ==hi == b").length, 0);
});

test("empty content is not a mark", () => {
  assert.equal(scanHighlights("a == == b").length, 0);
  assert.equal(scanHighlights("a ==== b").length, 0);
});

test("a === run is literal (delimiters are exactly two)", () => {
  assert.equal(scanHighlights("a ===x=== b").length, 0);
  assert.equal(scanHighlights("a ===x== b").length, 0);
  assert.equal(scanHighlights("a ==x=== b").length, 0);
});

test("an unmatched opener is literal", () => {
  assert.equal(scanHighlights("a ==hi b").length, 0);
});

test("marks are line-bounded", () => {
  assert.equal(scanHighlights("a ==hi\nbye== b").length, 0);
});

test("single = signs are untouched", () => {
  assert.equal(scanHighlights("a = b = c").length, 0);
});

test("nothing inside fenced code blocks", () => {
  assert.equal(scanHighlights("```\n==hi==\n```").length, 0);
});

test("nothing inside inline code spans", () => {
  assert.equal(scanHighlights("`==hi==` and ==ok==").length, 1);
});

test("a caller mask (the tree's indented-code ranges) wins", () => {
  assert.equal(scanHighlights("    ==hi==", [[4, 10]]).length, 0);
  assert.equal(scanHighlights("    ==hi==").length, 1);
});

test("frontmatter contents are not scanned", () => {
  assert.equal(scanHighlights("---\nt: ==hi==\n---\n==ok==").length, 1);
});

test("the first valid closer wins, the rest stays literal", () => {
  const r = scanHighlights("==a==b==");
  assert.equal(r.length, 1);
  assert.equal(r[0].content, "a");
});
