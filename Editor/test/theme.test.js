import { test } from "node:test";
import assert from "node:assert/strict";

import { proseColumn } from "../src/theme.js";

// The prose column (TAN-211): wide enough to use a large window, still
// capped, centred, and padded so the text never touches the pane edge.
test("the prose column is capped at 1100px, centred and padded", () => {
  const content = proseColumn;
  assert.equal(content.maxWidth, "1100px");
  assert.equal(content.margin, "0 auto");
  assert.equal(content.padding, "0 28px");
});
