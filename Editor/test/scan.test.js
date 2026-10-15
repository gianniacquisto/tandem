import { test } from "node:test";
import assert from "node:assert/strict";

import { scanWikilinks } from "../src/pure/scan.js";

const link = (r) => r.target;

test("a simple wikilink is found with exact offsets", () => {
  const text = "see [[note]] for more";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(r[0].from, 4);
  assert.equal(r[0].to, 12); // after the closing ]]
  assert.equal(text.slice(r[0].from, r[0].to), "[[note]]");
  assert.equal(r[0].content, "note");
  assert.equal(r[0].target, "note");
});

test("first ]] closes: text after the first ]] is literal", () => {
  const text = "[[a]]b]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(r[0].target, "a");
  assert.equal(r[0].to, 5);
});

test("first ]] closes even inside a nested-looking [[", () => {
  const text = "[[a[[b]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(r[0].target, "a[[b"); // the first ]] closes; target kept as written
});

test("unclosed [[ at end of line is not a link", () => {
  const text = "open [[note\nclosed [[ok]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "ok");
});

test("unclosed [[ at end of file is not a link", () => {
  const text = "dangling [[note";
  assert.equal(scanWikilinks(text).length, 0);
});

test("a ]] on a later line does not close (line-bounded scan)", () => {
  const text = "[[note\n]]";
  assert.equal(scanWikilinks(text).length, 0);
});

test("embeds ![[…]] are not links", () => {
  const text = "![[img]]";
  assert.equal(scanWikilinks(text).length, 0);
});

test("an embed does not consume a following real link", () => {
  const text = "![[img]] and [[note]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "note");
});

test("[[ inside a code fence is not a link", () => {
  const text = "before\n```\n[[note]]\n```\nafter [[real]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "real");
});

test("tilde fences are honoured too", () => {
  const text = "~~~\n[[note]]\n~~~\n[[real]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "real");
});

test("a backtick fence needs its own backticks to close (shorter fence does not close)", () => {
  const text = "````\n```\n[[note]]\n````\n[[real]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "real");
});

test("[[ inside inline code is not a link", () => {
  const text = "use `[[note]]` carefully and [[real]] too";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "real");
});

test("longer backtick spans mask the right runs", () => {
  const text = "``a [[x]] b`` and [[real]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "real");
});

test("unclosed inline code run does not mask the rest of the line", () => {
  const text = "half `open and [[real]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "real");
});

test("wikilinks in a leading frontmatter block are not links", () => {
  const text = "---\ntags: [[x]]\n---\nbody [[real]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "real");
});

test("an unterminated leading --- block is not frontmatter (its [[ are scanned)", () => {
  const text = "---\n[[real]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "real");
});

test("multiple links on one line, in order", () => {
  const text = "[[a]] mid [[b|B]] end";
  const r = scanWikilinks(text);
  assert.deepEqual(r.map(link), ["a", "b"]);
  assert.equal(r[1].display, "B");
});

test("same-note [[#Heading]] is not a link, but a following link is", () => {
  const text = "[[#Heading]] then [[note]]";
  const r = scanWikilinks(text);
  assert.equal(r.length, 1);
  assert.equal(link(r[0]), "note");
});
