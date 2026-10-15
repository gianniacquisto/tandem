import { test } from "node:test";
import assert from "node:assert/strict";

import { parseWikilink } from "../src/pure/grammar.js";

// Spec: `[[target[#fragment][|alias]]]` — split at the first `#`, then the
// first `|`; display text = alias if present, else fragment if present, else
// the target as written.

test("bare target: target as written is the display", () => {
  const r = parseWikilink("note");
  assert.equal(r.kind, "link");
  assert.equal(r.target, "note");
  assert.equal(r.fragment, undefined);
  assert.equal(r.alias, undefined);
  assert.equal(r.display, "note");
});

test("alias: [[note|alias]] displays the alias", () => {
  const r = parseWikilink("note|alias");
  assert.equal(r.kind, "link");
  assert.equal(r.target, "note");
  assert.equal(r.alias, "alias");
  assert.equal(r.display, "alias");
});

test("fragment: [[note#Heading]] displays the fragment", () => {
  const r = parseWikilink("note#Heading");
  assert.equal(r.kind, "link");
  assert.equal(r.target, "note");
  assert.equal(r.fragment, "Heading");
  assert.equal(r.display, "Heading");
});

test("fragment and alias: alias wins the display", () => {
  const r = parseWikilink("note#Heading|alias");
  assert.equal(r.kind, "link");
  assert.equal(r.target, "note");
  assert.equal(r.fragment, "Heading");
  assert.equal(r.alias, "alias");
  assert.equal(r.display, "alias");
});

test("split order: first # then first | (chained fragment)", () => {
  // `a#b#c|d` → target a, fragment b#c (everything between the first # and
  // the first |), alias d.
  const r = parseWikilink("a#b#c|d");
  assert.equal(r.kind, "link");
  assert.equal(r.target, "a");
  assert.equal(r.fragment, "b#c");
  assert.equal(r.alias, "d");
  assert.equal(r.display, "d");
});

test("split order: no # → the first | splits target from alias", () => {
  const r = parseWikilink("a|b|c");
  assert.equal(r.kind, "link");
  assert.equal(r.target, "a");
  assert.equal(r.alias, "b|c");
  assert.equal(r.display, "b|c");
});

test("a | before any # stays in the target (grammar: target[#fragment][|alias])", () => {
  const r = parseWikilink("a|b#c");
  assert.equal(r.kind, "link");
  assert.equal(r.target, "a|b");
  assert.equal(r.fragment, "c");
  assert.equal(r.display, "c");
});

test("target is kept as written (spaces preserved, resolution is the resolver's job)", () => {
  const r = parseWikilink("my note");
  assert.equal(r.kind, "link");
  assert.equal(r.target, "my note");
  assert.equal(r.display, "my note");
});
