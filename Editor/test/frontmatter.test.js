import { test } from "node:test";
import assert from "node:assert/strict";

import { frontmatterRange } from "../src/pure/frontmatter.js";

// Core parity (BikeshedOperations.frontmatterEnd): ONLY an exact `---`
// line-one delimiter and an exact `---` closer count. No trailing
// whitespace, no `...` closer (the editor and the core must agree on what
// is opaque — a `...`-closed block is plain text whose wikilinks the core
// rewrites on rename). A lone trailing `\r` is tolerated (CRLF).

test("a leading, properly terminated --- block is frontmatter", () => {
  assert.deepEqual(frontmatterRange("---\na: 1\n---\nbody"), { from: 0, to: 13 });
});

test("the closer with no trailing newline still terminates the block", () => {
  assert.deepEqual(frontmatterRange("---\na: 1\n---"), { from: 0, to: 12 });
});

test("a ... closer is NOT frontmatter (core: exact --- only)", () => {
  assert.equal(frontmatterRange("---\na: 1\n...\nbody"), null);
});

test("trailing whitespace is not a delimiter (core: exact --- only)", () => {
  assert.equal(frontmatterRange("--- \na: 1\n---\nbody"), null);
  assert.equal(frontmatterRange("---\na: 1\n--- \nbody"), null);
  assert.equal(frontmatterRange("----\na: 1\n---\nbody"), null);
});

test("a CRLF delimiter is frontmatter (core parity: one trailing \\r)", () => {
  assert.deepEqual(frontmatterRange("---\r\na: 1\r\n---\r\nbody"), { from: 0, to: 16 });
});

test("a lone --- (unterminated) is not frontmatter", () => {
  assert.equal(frontmatterRange("---"), null);
  assert.equal(frontmatterRange("---\n[[real]]"), null);
});
