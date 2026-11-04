import { test } from "node:test";
import assert from "node:assert/strict";

import { proseColumn } from "../src/theme.js";

// The prose column (TAN-211, TAND-8): a comfortable reading measure,
// centred, and padded so the text never touches the pane edge.
test("the prose column is a centred reading measure with soft wrap", () => {
  const content = proseColumn;
  assert.equal(content.maxWidth, "720px");
  assert.equal(content.margin, "0 auto");
  assert.equal(content.padding, "0 28px");
  assert.equal(content.overflowWrap, "anywhere");
});
