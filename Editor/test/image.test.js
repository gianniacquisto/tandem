import { test } from "node:test";
import assert from "node:assert/strict";

import { resolveImageTarget } from "../src/pure/image.js";

// Spec: Images & attachments — local images resolved relative to the
// containing note and confined to the bikeshed; remote http(s) targets are
// never fetched (rendered as the broken-image placeholder).

test("a bare relative path resolves against the note's folder (bikeshed root note)", () => {
  assert.deepEqual(resolveImageTarget("img.png", "n.md"), {
    kind: "bikeshed",
    path: "img.png",
  });
});

test("a relative path resolves against a nested note's folder", () => {
  assert.deepEqual(resolveImageTarget("sub/img.png", "a/n.md"), {
    kind: "bikeshed",
    path: "a/sub/img.png",
  });
});

test("a ../ step climbs within the bikeshed", () => {
  assert.deepEqual(resolveImageTarget("../x.png", "a/n.md"), {
    kind: "bikeshed",
    path: "x.png",
  });
});

test("a ./ step is the identity", () => {
  assert.deepEqual(resolveImageTarget("./img.png", "a/n.md"), {
    kind: "bikeshed",
    path: "a/img.png",
  });
});

test("a leading / is bikeshed-root-relative", () => {
  assert.deepEqual(resolveImageTarget("/abs.png", "a/n.md"), {
    kind: "bikeshed",
    path: "abs.png",
  });
});

test("escaping the bikeshed root is refused", () => {
  assert.deepEqual(resolveImageTarget("../../x.png", "a/n.md"), { kind: "escape" });
  assert.deepEqual(resolveImageTarget("../../..", "a/b/n.md"), { kind: "escape" });
});

test("a path that climbs back to exactly the bikeshed root stays inside", () => {
  assert.deepEqual(resolveImageTarget("../a/b/c/../../..", "a/n.md"), {
    kind: "bikeshed",
    path: "",
  });
});

test("remote http(s) targets are remote (never fetched)", () => {
  assert.deepEqual(resolveImageTarget("http://x.example/y.png", "n.md"), {
    kind: "remote",
    src: "http://x.example/y.png",
  });
  assert.deepEqual(resolveImageTarget("https://x.example/y.png", "a/n.md"), {
    kind: "remote",
    src: "https://x.example/y.png",
  });
});

test("a dot segment in the middle is normalised", () => {
  assert.deepEqual(resolveImageTarget("a/./b/../c.png", "n.md"), {
    kind: "bikeshed",
    path: "a/c.png",
  });
});
