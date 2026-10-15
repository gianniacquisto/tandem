import { test } from "node:test";
import assert from "node:assert/strict";

import { scanMath } from "../src/pure/math.js";

// Spec: KaTeX math (inline + display block form, Obsidian-style trigger rules,
// parse errors leave raw math visible — error handling is the renderer's job).
// Concrete trigger rules (research verdict, settled): inline math = `$…$`
// where the opener is not `$$` and the content's edges are non-space
// (trimmed non-empty); display = block form `$$…$$` on its own lines.
// Cross-cutting: nothing is processed inside code fences or inline code.

const ranges = (text) => scanMath(text).map((m) => [m.from, m.to]);

test("inline math between two $ with non-space edges", () => {
  const r = scanMath("a $x + y$ b");
  assert.equal(r.length, 1);
  assert.equal(r[0].kind, "inline");
  assert.equal(r[0].content, "x + y");
  assert.deepEqual(ranges("a $x + y$ b"), [[2, 9]]);
});

test("a currency $ does not start math (no second $ on the line)", () => {
  assert.equal(scanMath("it costs $5 today").length, 0);
});

test("a currency pair with spaces at the edges is not math", () => {
  assert.equal(scanMath("costs $5 and $10 total").length, 0);
});

test("space right after the opener is not inline math", () => {
  assert.equal(scanMath("a $ x$ b").length, 0);
});

test("space right before the closer is not inline math", () => {
  assert.equal(scanMath("a $x $ b").length, 0);
});

test("empty math is not math", () => {
  assert.equal(scanMath("a $$ b").length, 0);
  assert.equal(scanMath("a $ $ b").length, 0);
});

test("inline math is line-bounded", () => {
  assert.equal(scanMath("a $x\ny$ b").length, 0);
});

test("single-line display math $$…$$", () => {
  const r = scanMath("$$e = mc^2$$");
  assert.equal(r.length, 1);
  assert.equal(r[0].kind, "display");
  assert.equal(r[0].content, "e = mc^2");
});

test("multi-line display math, $$ on its own lines", () => {
  const text = "before\n$$\nx = 1\n$$\nafter";
  const r = scanMath(text);
  assert.equal(r.length, 1);
  assert.equal(r[0].kind, "display");
  assert.equal(r[0].content, "x = 1");
  // range covers from the opening $$ to the closing $$ (inclusive)
  assert.equal(text.slice(r[0].from, r[0].to), "$$\nx = 1\n$$");
});

test("an unclosed display $$ is literal", () => {
  assert.equal(scanMath("$$\nx = 1\n").length, 0);
});

test("a $$ opener line with other text is not display math", () => {
  assert.equal(scanMath("money $$ stuff\n$$\n").length, 0);
});

test("a $$ closer line with other text is not a closer", () => {
  assert.equal(scanMath("$$\nx = 1 $$\n").length, 0);
});

test("three-dollar runs are not delimiters (a real $x$ still parses)", () => {
  assert.equal(scanMath("$$$x$$$").length, 0);
  const r = scanMath("$x$ inside $$$");
  assert.equal(r.length, 1);
  assert.equal(r[0].content, "x");
  assert.equal(r[0].from, 0);
});

test("math in a code fence is literal", () => {
  const r = scanMath("```\n$x$\n```\nreal $y$");
  assert.equal(r.length, 1);
  assert.equal(r[0].content, "y");
});

test("math delimiters inside inline code are literal", () => {
  const r = scanMath("`$x$` but $y$");
  assert.equal(r.length, 1);
  assert.equal(r[0].content, "y");
});

test("a math delimiter cannot sit inside an inline code span", () => {
  // opener inside the span → no math; the $ outside pairs with nothing
  assert.equal(scanMath("`$`x$").length, 0);
});

test("two inline maths on one line", () => {
  const r = scanMath("$a$ mid $b$");
  assert.equal(r.length, 2);
  assert.deepEqual(r.map((m) => m.content), ["a", "b"]);
});

test("masks (e.g. wikilink ranges) are never scanned", () => {
  const text = "[[a $b$ c]] and $d$";
  const masked = scanMath(text, [[0, 13]]);
  assert.equal(masked.length, 1);
  assert.equal(masked[0].content, "d");
});

test("a later opener is found when the first has no valid closer", () => {
  // `$ $a$`: the first $ has no valid closer (content would start with a
  // space); the real math is `$a$` starting at index 2.
  const text = "$ $a$";
  const r = scanMath(text);
  assert.equal(r.length, 1);
  assert.equal(r[0].content, "a");
  assert.equal(r[0].from, 2);
  assert.equal(r[0].to, 5);
});
