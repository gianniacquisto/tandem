import { test } from "node:test";
import assert from "node:assert/strict";

import { parseWikilink, NOT_LINK } from "../src/pure/grammar.js";

// Spec: "Not links (literal text): unclosed `[[` at end of line/file, empty
// or whitespace-only target, the same-note form `[[#Heading]]`, and embeds
// `![[…]]`." (unclosed + embeds are decided by the scanner, see scan.test.js;
// the target-shape not-links are decided here.)

test("empty target is not a link", () => {
  const r = parseWikilink("");
  assert.equal(r.kind, "not-link");
  assert.equal(r.reason, NOT_LINK.EMPTY_TARGET);
});

test("whitespace-only target is not a link", () => {
  const r = parseWikilink("   ");
  assert.equal(r.kind, "not-link");
  assert.equal(r.reason, NOT_LINK.EMPTY_TARGET);
});

test("target empty with only an alias is not a link", () => {
  const r = parseWikilink("|alias");
  assert.equal(r.kind, "not-link");
});

test("same-note form [[#Heading]] is not a link", () => {
  const r = parseWikilink("#Heading");
  assert.equal(r.kind, "not-link");
  assert.equal(r.reason, NOT_LINK.SAME_NOTE);
});

test("same-note form with alias is not a link", () => {
  const r = parseWikilink("#Heading|alias");
  assert.equal(r.kind, "not-link");
  assert.equal(r.reason, NOT_LINK.SAME_NOTE);
});

test("[[#]] (empty target, empty fragment) is not a link", () => {
  const r = parseWikilink("#");
  assert.equal(r.kind, "not-link");
  assert.equal(r.reason, NOT_LINK.EMPTY_TARGET);
});
