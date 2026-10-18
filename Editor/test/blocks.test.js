import { test } from "node:test";
import assert from "node:assert/strict";

import { markdownLanguage } from "@codemirror/lang-markdown";
import { planPreview } from "../src/blocks.js";

const plan = (text, opts = {}) =>
  planPreview({
    text,
    tree: markdownLanguage.parser.parse(text),
    notes: opts.notes ?? [],
    notePath: opts.notePath ?? "n.md",
  });

const widgetAt = (p, from) =>
  p.replaces.find((r) => r.from === from && r.widget)?.widget;
const emptyAt = (p, from, to) =>
  p.replaces.some((r) => r.from === from && r.to === to && !r.widget);
const blockOf = (p, kind) => p.blocks.find((b) => b.widget.kind === kind);
const lineClass = (p, from) => p.lines.find((l) => l.from === from)?.className;

test("ATX heading: marker + following space hidden, line class by level", () => {
  const p = plan("# One\n## Two");
  assert.ok(emptyAt(p, 0, 2)); // "# "
  assert.equal(lineClass(p, 0), "tandem-h1");
  assert.ok(emptyAt(p, 6, 9)); // "## "
  assert.equal(lineClass(p, 6), "tandem-h2");
});

test("ATX closing sequence is hidden (with its space)", () => {
  const p = plan("# H #");
  // "# " [0,2) hidden, " #": space + mark [3,5) hidden
  assert.ok(p.replaces.some((r) => r.from === 0 && r.to === 2 && !r.widget));
  assert.ok(p.replaces.some((r) => r.from === 3 && r.to === 5 && !r.widget));
});

test("setext heading: underline line gone entirely, text line classed", () => {
  const p = plan("Title\n===\n\nnext");
  assert.equal(lineClass(p, 0), "tandem-h1");
  // the whole underline line incl. its newline is replaced: [6,10)
  assert.ok(p.replaces.some((r) => r.from === 6 && r.to === 10 && !r.widget));
});

test("emphasis and strikethrough: markers hidden, inner span classed", () => {
  const p = plan("**bold** and ~~strike~~ and *em*");
  // **bold**: marks [0,2) [6,8); span [2,6) tandem-strong
  assert.ok(emptyAt(p, 0, 2) && emptyAt(p, 6, 8));
  assert.ok(p.spans.some((s) => s.from === 2 && s.to === 6 && s.className === "tandem-strong"));
  // ~~strike~~ at 13: marks [13,15) [21,23); span [15,21)
  assert.ok(emptyAt(p, 13, 15) && emptyAt(p, 21, 23));
  assert.ok(p.spans.some((s) => s.from === 15 && s.to === 21 && s.className === "tandem-strike"));
});

test("inline code: backticks hidden, inner span classed", () => {
  const p = plan("use `code` here");
  assert.ok(emptyAt(p, 4, 5) && emptyAt(p, 9, 10));
  assert.ok(p.spans.some((s) => s.from === 5 && s.to === 9 && s.className === "tandem-code"));
});

test("bullets: unordered dot, ordered number, nested depth class + hidden indent", () => {
  const p = plan("- a\n- b\n  - c\n1. one\n2. two");
  assert.deepEqual(widgetAt(p, 0), { kind: "bullet" });
  assert.deepEqual(widgetAt(p, 4), { kind: "bullet" });
  // nested "- c": leading indent [8,10) hidden, depth-1 class, bullet at 10
  assert.ok(emptyAt(p, 8, 10));
  assert.deepEqual(widgetAt(p, 10), { kind: "bullet" });
  assert.match(lineClass(p, 8), /tandem-li-d1/);
  // ordered
  assert.deepEqual(widgetAt(p, 14), { kind: "bullet", number: "1" });
  assert.deepEqual(widgetAt(p, 21), { kind: "bullet", number: "2" });
});

test("task items: no bullet, checkbox widget, done class when checked", () => {
  const p = plan("- [ ] open\n- [x] done");
  assert.equal(widgetAt(p, 0), undefined); // no bullet
  assert.deepEqual(widgetAt(p, 2), { kind: "checkbox", checked: false, markerFrom: 2 });
  assert.deepEqual(widgetAt(p, 13), { kind: "checkbox", checked: true, markerFrom: 13 });
  assert.match(lineClass(p, 11), /tandem-task-done/);
  assert.equal(lineClass(p, 0) ?? "", "");
});

test("blockquote: marks hidden, panel classes first/last", () => {
  const p = plan("> one\n> two");
  assert.ok(emptyAt(p, 0, 2) && emptyAt(p, 6, 8));
  assert.match(lineClass(p, 0), /tandem-quote-first/);
  assert.match(lineClass(p, 6), /tandem-quote-last/);
});

test("table: block widget with cells, source lines replaced as a unit", () => {
  const text = "| a | b |\n|---|---|\n| 1 | 2 |";
  const p = plan(text);
  const t = blockOf(p, "table");
  assert.deepEqual(t.widget.header, ["a", "b"]);
  assert.deepEqual(t.widget.rows, [["1", "2"]]);
  assert.equal(t.at, 0);
  // whole block incl. trailing newline replaced
  assert.ok(p.replaces.some((r) => r.from === 0 && r.to === text.length && !r.widget));
  // a wikilink inside a cell is swallowed by the table replace
  const p2 = plan("| [[x]] |\n|---|\n| y |");
  assert.equal(p2.replaces.filter((r) => r.widget?.kind === "wikilink").length, 0);
  assert.ok(blockOf(p2, "table"));
});

test("fenced code: fence lines hidden, code lines classed, tag widget carries info", () => {
  const text = "```swift\nlet x = 1\n```\nnext";
  const p = plan(text);
  assert.ok(emptyAt(p, 0, 9)); // "```swift\n"
  assert.ok(emptyAt(p, 19, 22)); // closing "```"
  assert.match(lineClass(p, 9), /tandem-code-line/);
  assert.match(lineClass(p, 19), /tandem-code-end/);
  const tag = blockOf(p, "codelangtag");
  assert.equal(tag.at, 9);
  assert.equal(tag.widget.info, "swift");
});

test("fenced code without info: empty tag; empty block still gets a tag", () => {
  const p = plan("```\nx\n```");
  assert.equal(blockOf(p, "codelangtag").widget.info, "");
  const p2 = plan("```\n```");
  assert.equal(blockOf(p2, "codelangtag").widget.info, "");
});

test("indented code: lines classed, no markers touched", () => {
  const p = plan("para\n\n    code line\n    more");
  const lines = p.lines.filter((l) => l.className.includes("tandem-codeblock"));
  assert.equal(lines.length, 2);
  assert.match(lines[0].className, /tandem-codeblock-first/);
  assert.match(lines[1].className, /tandem-codeblock-last/);
});

test("horizontal rule: line replaced, hr block", () => {
  const p = plan("a\n\n---\n\nb");
  const hr = blockOf(p, "hr");
  assert.ok(hr);
  assert.equal(hr.at, 3);
});

test("wikilinks: resolved vs broken against the bikeshed, display text per grammar", () => {
  const p = plan("see [[note]] [[missing#H|A]]", { notes: ["note.md"] });
  const w = p.replaces.filter((r) => r.widget?.kind === "wikilink");
  assert.equal(w.length, 2);
  assert.deepEqual(w[0].widget, { kind: "wikilink", target: "note", display: "note", broken: false });
  assert.deepEqual(w[1].widget, { kind: "wikilink", target: "missing", display: "A", broken: true });
});

test("wikilinks in code (fenced, indented, inline) are not links (tree masks)", () => {
  const notes = ["x.md"];
  const p = plan("```\n[[x]]\n```\n\n    [[x]]\n\n`[[x]]`", { notes });
  assert.equal(p.replaces.filter((r) => r.widget?.kind === "wikilink").length, 0);
});

test("inline math: replace with math widget; currency is not math", () => {
  const p = plan("a $e=mc^2$ b and $5 c");
  const m = p.replaces.find((r) => r.widget?.kind === "math");
  assert.equal(m.widget.content, "e=mc^2");
  assert.equal(p.replaces.filter((r) => r.widget?.kind === "math").length, 1);
});

test("display math: block widget, whole lines replaced", () => {
  const text = "before\n$$\nx = 1\n$$\nafter";
  const p = plan(text);
  const d = blockOf(p, "displaymath");
  assert.equal(d.widget.content, "x = 1");
  assert.equal(d.at, 7); // line start of the opening $$
  // three lines (incl. trailing newline) replaced: [7, 19)
  assert.ok(p.replaces.some((r) => r.from === 7 && r.to === 19 && !r.widget));
});

test("a wikilink inside a display block is swallowed by the math replace", () => {
  const p = plan("$$\n[[x]]\n$$", { notes: ["x.md"] });
  assert.equal(p.replaces.filter((r) => r.widget?.kind === "wikilink").length, 0);
  assert.ok(blockOf(p, "displaymath"));
});

test("whole-line image: block widget with resolved src fields", () => {
  const p = plan("text\n\n![alt](img/p.png)\n\nmore", { notePath: "a/n.md" });
  const img = blockOf(p, "image");
  assert.deepEqual(img.widget, { kind: "image", src: "img/p.png", alt: "alt", notePath: "a/n.md" });
});

test("mid-line image: inline image replace, not a block", () => {
  const p = plan("see ![a](i/x.png) here");
  const w = p.replaces.find((r) => r.widget?.kind === "inlineimage");
  assert.equal(w.widget.src, "i/x.png");
  assert.equal(blockOf(p, "image"), undefined);
});

test("markdown link: marks + url hidden, span carries the href", () => {
  const p = plan("[text](https://e.com/a.md)");
  const href = p.spans.find((s) => s.className === "tandem-link");
  assert.equal(href.dataHref, "https://e.com/a.md");
  assert.equal(href.from, 1);
  assert.equal(href.to, 5);
  assert.ok(p.replaces.some((r) => r.from === 5 && r.to === 26 && !r.widget)); // "] (url)"...
});

test("bare and angled autolinks: link spans with hrefs", () => {
  const p = plan("<https://a.com/x> and https://b.com/y");
  const hrefs = p.spans.filter((s) => s.className === "tandem-link").map((s) => s.dataHref);
  assert.deepEqual(hrefs, ["https://a.com/x", "https://b.com/y"]);
  // angle brackets hidden
  assert.ok(emptyAt(p, 0, 1) && emptyAt(p, 16, 17));
});

test("frontmatter: stripped block + placeholder, its wikilinks not links", () => {
  const text = "---\nt: [[x]]\n---\nbody [[real]]";
  const p = plan(text, { notes: ["real.md", "x.md"] });
  assert.ok(blockOf(p, "frontmatter"));
  assert.ok(p.replaces.some((r) => r.from === 0 && r.to === 17 && !r.widget));
  const w = p.replaces.filter((r) => r.widget?.kind === "wikilink");
  assert.equal(w.length, 1);
  assert.equal(w[0].widget.target, "real");
});

test("frontmatter does not abort the walk: structure after it is planned", () => {
  // Regression: the skip guard was `node.from < fmRange.to`, which matched
  // the Document root (from 0) and aborted the whole walk — a note with
  // frontmatter got zero structural decorations.
  const text = "---\nt: x\n---\n# After\n\n**bold**";
  const p = plan(text);
  assert.ok(blockOf(p, "frontmatter")); // the strip itself still works
  assert.equal(lineClass(p, 13), "tandem-h1");
  assert.ok(emptyAt(p, 13, 15)); // "# " hidden
  assert.ok(
    p.spans.some((s) => s.from === 24 && s.to === 28 && s.className === "tandem-strong")
  );
});

test("no overlapping replaces in any plan", () => {
  const texts = [
    "# H **b** `c` [[x]] $y$\n",
    "- [ ] - [x] done **[[a|b]]**\n",
    "> **[[q]]** and $z$\n",
    "| [[t]] | **u** |\n|---|---|\n| `v` | $w$ |",
    "---\na: 1\n---\n# H\n[[x]] `c`\n",
    "```\n[[x]] $y$\n```\n",
    "![a](i.png)\n\n![b](j.png) inline ![c](k.png)",
  ];
  for (const text of texts) {
    const p = plan(text, { notes: ["a.md", "b.md"] });
    const rs = [...p.replaces].sort((a, b) => a.from - b.from);
    for (let i = 1; i < rs.length; i++)
      assert.ok(rs[i].from >= rs[i - 1].to, `overlap in ${JSON.stringify(text)}`);
  }
});
