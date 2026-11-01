import { test } from "node:test";
import assert from "node:assert/strict";

import { renderDiagram } from "../src/mermaid.js";

// The render path (src/mermaid.js): lazy mermaid import, one initialize, and
// an async render per diagram. These tests drive it with an injected
// renderer, so no DOM and no mermaid import are needed: the module's own
// lazy `import("mermaid")` never runs here.
//
// Contract: a resolved render mounts the SVG into the element; a rejection
// (mermaid's parse error) leaves the raw source visible under
// `.tandem-mermaid-error` in monospace, the same fallback display math has;
// and a module-level cache keyed by source plus theme answers a repeat
// render without calling the renderer again (a re-decoration must not
// re-render the same diagram).

function fakeElement() {
  const classes = new Set();
  return {
    innerHTML: "",
    textContent: "",
    classList: {
      add: (c) => classes.add(c),
      remove: (c) => classes.delete(c),
      contains: (c) => classes.has(c),
    },
  };
}

const countingRenderer = (svgFor) => {
  const calls = [];
  const render = async (source, theme) => {
    calls.push([source, theme]);
    return svgFor(source, theme);
  };
  render.calls = calls;
  return render;
};

test("a resolved render mounts the SVG into the element", async () => {
  const el = fakeElement();
  const svg = '<svg id="m1"><g/></svg>';
  const out = await renderDiagram(el, "graph TD\nA-->B", "day", {
    render: async () => svg,
    cache: new Map(),
  });
  assert.equal(out, svg);
  assert.equal(el.innerHTML, svg);
  assert.equal(el.classList.contains("tandem-mermaid-error"), false);
});

test("a rejected render leaves the raw source under the error class", async () => {
  const el = fakeElement();
  const source = "graph TD\nA-->";
  const out = await renderDiagram(el, source, "day", {
    render: async () => {
      throw new Error("Parse error on line 2");
    },
    cache: new Map(),
  });
  assert.equal(out, null);
  assert.equal(el.textContent, source);
  assert.equal(el.innerHTML, "");
  assert.equal(el.classList.contains("tandem-mermaid-error"), true);
});

test("the cache answers the same source plus theme without a second render", async () => {
  const cache = new Map();
  const render = countingRenderer(() => "<svg/>");
  const first = fakeElement();
  const second = fakeElement();
  await renderDiagram(first, "graph TD\nA-->B", "day", { render, cache });
  await renderDiagram(second, "graph TD\nA-->B", "day", { render, cache });
  assert.equal(render.calls.length, 1);
  assert.equal(second.innerHTML, "<svg/>");
});

test("a different theme renders again against the new palette", async () => {
  const cache = new Map();
  const render = countingRenderer((_source, theme) => `<svg data-theme="${theme}"/>`);
  const el = fakeElement();
  await renderDiagram(el, "graph TD\nA-->B", "day", { render, cache });
  assert.equal(el.innerHTML, '<svg data-theme="day"/>');
  await renderDiagram(el, "graph TD\nA-->B", "night", { render, cache });
  assert.equal(render.calls.length, 2);
  assert.equal(el.innerHTML, '<svg data-theme="night"/>');
});

test("a different source is a different cache entry", async () => {
  const cache = new Map();
  const render = countingRenderer((source) => `<svg data-src="${source.length}"/>`);
  await renderDiagram(fakeElement(), "graph TD\nA-->B", "day", { render, cache });
  await renderDiagram(fakeElement(), "graph LR\nA-->B", "day", { render, cache });
  assert.equal(render.calls.length, 2);
});
