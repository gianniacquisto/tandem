import { test } from "node:test";
import assert from "node:assert/strict";

import { scanDocument } from "../src/pure/index.js";

test("math inside a wikilink target is not math (target kept as written)", () => {
  const r = scanDocument("[[a $b$ c]]");
  assert.equal(r.wikilinks.length, 1);
  assert.equal(r.wikilinks[0].target, "a $b$ c");
  assert.equal(r.math.length, 0);
});

test("a wikilink inside a multi-line display block is not a link", () => {
  const r = scanDocument("$$\n[[x]]\n$$");
  assert.equal(r.wikilinks.length, 0);
  assert.equal(r.math.length, 1);
  assert.equal(r.math[0].kind, "display");
  assert.equal(r.math[0].content, "[[x]]");
});

test("a wikilink inside a single-line display block is not a link", () => {
  const r = scanDocument("$$[[x]]$$");
  assert.equal(r.wikilinks.length, 0);
  assert.equal(r.math.length, 1);
  assert.equal(r.math[0].kind, "display");
});

test("both kinds coexist when disjoint", () => {
  const r = scanDocument("[[x]] and $y$\n$$z$$");
  assert.equal(r.wikilinks.length, 1);
  assert.equal(r.math.length, 2);
  assert.deepEqual(r.math.map((m) => m.kind), ["inline", "display"]);
});

test("a $$…$$ at the end of a text line is literal (display is block form, on its own lines)", () => {
  const r = scanDocument("text $$z$$ more");
  assert.equal(r.math.length, 0);
});

test("frontmatter is reported and its contents are not scanned", () => {
  const text = "---\nt: [[x]]\n---\n[[real]]";
  const r = scanDocument(text);
  assert.equal(r.frontmatter.from, 0);
  assert.ok(r.frontmatter.to > 0);
  assert.equal(r.wikilinks.length, 1);
  assert.equal(r.wikilinks[0].target, "real");
});
