import { test } from "node:test";
import assert from "node:assert/strict";

import { resolveWikilink } from "../src/pure/resolve.js";

// Spec: Wikilink model — Resolution.
// Bare target: case-insensitive basename (filename minus extension) match,
// resolving iff EXACTLY ONE note matches; zero or multiple → broken.
// Qualified target (contains /): vault-root-relative path, same exactly-one
// rule. Duplicated names (incl. case-variant duplicates) → broken, never a
// silently chosen match.

test("bare target resolves case-insensitively to the unique note", () => {
  const r = resolveWikilink("my note", ["Notes/My note.md"]);
  assert.equal(r.status, "resolved");
  assert.equal(r.path, "Notes/My note.md");
});

test("zero matches → broken", () => {
  assert.equal(resolveWikilink("ghost", ["a.md"]).status, "broken");
  assert.equal(resolveWikilink("a", []).status, "broken");
});

test("multiple basename matches (different folders) → broken, never a silent pick", () => {
  assert.equal(resolveWikilink("a", ["x/a.md", "y/a.md"]).status, "broken");
});

test("case-variant duplicates are ambiguous → broken", () => {
  assert.equal(resolveWikilink("a", ["a.md", "A.md"]).status, "broken");
});

test("qualified target is vault-root-relative and case-insensitive", () => {
  const r = resolveWikilink("FOLDER/NOTE", ["folder/note.md"]);
  assert.equal(r.status, "resolved");
  assert.equal(r.path, "folder/note.md");
});

test("qualified target with no matching path → broken (even if the basename exists elsewhere)", () => {
  assert.equal(resolveWikilink("folder/note", ["other/note.md"]).status, "broken");
});

test("qualified target with case-variant duplicates → broken", () => {
  assert.equal(resolveWikilink("folder/note", ["folder/note.md", "Folder/note.md"]).status, "broken");
});

test("a bare target that names a folder-only prefix → broken", () => {
  assert.equal(resolveWikilink("folder", ["folder/note.md"]).status, "broken");
});

test("basename is the filename minus its .md extension (case-insensitive extension)", () => {
  assert.equal(resolveWikilink("a", ["A.MD"]).status, "resolved");
  assert.equal(resolveWikilink("a.b", ["a.b.md"]).status, "resolved");
  assert.equal(resolveWikilink("a", ["a.b.md"]).status, "broken");
  // an inner .md is not the extension
  assert.equal(resolveWikilink("a.md", ["a.md.bak.md"]).status, "broken");
});

test("a target written with an explicit .md does not match (targets name notes by name, as written)", () => {
  assert.equal(resolveWikilink("a.md", ["a.md"]).status, "broken");
  assert.equal(resolveWikilink("folder/note.md", ["folder/note.md"]).status, "broken");
});

test("the path is returned exactly as on disk (case preserved)", () => {
  const r = resolveWikilink("nOte", ["NoTe.md"]);
  assert.equal(r.status, "resolved");
  assert.equal(r.path, "NoTe.md");
});
