// Live-preview decoration planner (spec: Editor — inline live preview).
//
// Pure (no DOM): given the document text, the incremental @lezer/markdown
// tree, the bridge's note file set, and the open note's path, it plans the
// decoration set the CM6 layer renders:
//   - replaces: inline Decoration.replace ranges — marker ranges with a
//     widget (wikilink, checkbox, bullet, math) or empty (bare markers);
//   - spans: inline mark ranges (emphasis, inline code, markdown links,
//     autolinks);
//   - lines: per-line classes (headings, lists, blockquotes, code, tasks);
//   - blocks: block widgets — tables, display math, whole-line images,
//     frontmatter, code language tag, horizontal rule. Block decorations
//     come from this StateField plan, never a ViewPlugin (spec invariant).
//
// Invariants: the plan is view-only (invariant 1); every line height is
// carried by a class, never by token visibility (invariant 2). Nothing is
// processed inside code — the tree's code ranges (fenced, indented, inline)
// mask the wikilink/math scanners (core parity: the Swift scanner masks the
// CommonMark parser's code ranges).

import { scanWikilinks } from "./pure/scan.js";
import { scanMath } from "./pure/math.js";
import { resolveWikilink } from "./pure/resolve.js";
import { frontmatterRange } from "./pure/frontmatter.js";

// -- line bookkeeping -------------------------------------------------------

function lineStarts(text) {
  const starts = [0];
  let i = text.indexOf("\n");
  while (i !== -1) {
    starts.push(i + 1);
    i = text.indexOf("\n", i + 1);
  }
  return starts;
}

function lineIndex(starts, pos) {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= pos) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

function makeLines(text, starts) {
  return {
    count: starts.length,
    from(i) {
      return starts[i];
    },
    to(i) {
      return i + 1 < starts.length ? starts[i + 1] - 1 : text.length;
    },
    indexAt(pos) {
      return lineIndex(starts, pos);
    },
    // [from, to) of line i, plus its trailing newline when present.
    withBreak(i) {
      const t = this.to(i);
      return [starts[i], text[t] === "\n" ? t + 1 : t];
    },
  };
}

// The display range a block widget replaces: the whole lines from the line
// containing `from` through the line containing `to - 1` (trailing newline
// included) — the source lines vanish from flow, the widget takes their place.
function blockLineRange(lines, text, from, to) {
  const a = lines.indexAt(from);
  const b = lines.indexAt(to - 1);
  return [lines.from(a), lines.withBreak(b)[1]];
}

const inRange = (pos, r) => (r ? pos >= r[0] && pos < r[1] : false);
const overlaps = (a, b, c, d) => a < d && c < b;

// -- plan -------------------------------------------------------------------

export function planPreview({ text, tree, notes = [], notePath = "" }) {
  const starts = lineStarts(text);
  const lines = makeLines(text, starts);
  const fm = frontmatterRange(text);
  const fmRange = fm ? [fm.from, fm.to] : null;

  const replaces = [];
  const spans = [];
  const lineClasses = new Map(); // line index -> Set of class names
  const blocks = [];

  const addReplace = (from, to, widget) =>
    replaces.push(widget ? { from, to, widget } : { from, to });
  const addLineClass = (lineIdx, name) => {
    if (!lineClasses.has(lineIdx)) lineClasses.set(lineIdx, new Set());
    lineClasses.get(lineIdx).add(name);
  };

  // Tree-derived code ranges: the scanner exemption set (fenced, indented,
  // inline). Computed before the scans so the pure scanners never see code.
  const codeRanges = [];
  if (tree) {
    (function collectCode(node) {
      if (
        node.name === "FencedCode" ||
        node.name === "CodeBlock" ||
        node.name === "InlineCode"
      )
        codeRanges.push([node.from, node.to]);
      // childrenOf walks a fresh cursor per node, so a deep subtree can't
      // strand the walk (the same hazard descend() had).
      for (const c of childrenOf(node)) collectCode(c);
    })(tree.cursor().node);
  }

  // 1+2. Wikilinks + math: the pure scanners with the tree's code ranges
  //    masked in (pure/index.js's fixed-point ordering — links first, math
  //    with the link ranges masked, then a re-pass of both with each other's
  //    ranges — extended with codeRanges so nothing inside code is scanned).
  const mask = (extra) => [...codeRanges, ...extra];
  const ranges = (items) => items.map((i) => [i.from, i.to]);
  let wikilinks = scanWikilinks(text, mask([]));
  let math = scanMath(text, mask(ranges(wikilinks)));
  wikilinks = scanWikilinks(text, mask(ranges(math)));
  math = scanMath(text, mask(ranges(wikilinks)));
  for (const w of wikilinks) {
    if (fmRange && w.from < fmRange[1]) continue;
    const res = resolveWikilink(w.target, notes);
    addReplace(w.from, w.to, {
      kind: "wikilink",
      target: w.target,
      display: w.display,
      broken: res.status !== "resolved",
    });
  }

  // Math (Obsidian-style triggers; wikilink ranges masked by the re-pass).
  for (const m of math) {
    if (fmRange && m.from < fmRange[1]) continue;
    if (m.kind === "inline") {
      addReplace(m.from, m.to, { kind: "math", content: m.content });
    } else {
      const [bf, bt] = blockLineRange(lines, text, m.from, m.to);
      addReplace(bf, bt);
      blocks.push({ at: bf, side: 0, widget: { kind: "displaymath", content: m.content } });
    }
  }

  // 3. Structural decorations from the tree.
  if (tree) {
    const ctx = {
      text,
      lines,
      fmRange,
      replaces,
      spans,
      blocks,
      addReplace,
      addLineClass,
      codeRanges,
      inTaskItem: 0,
      listKind: [],
      inQuote: 0,
      notePath,
    };
    const cur = tree.cursor();
    visit(cur, ctx);
  }

  // 4. Frontmatter: stripped from the preview (never parsed), shown as a
  //    designed placeholder row.
  if (fm) {
    addReplace(0, fm.to);
    blocks.push({ at: 0, side: 0, widget: { kind: "frontmatter" } });
  }

  // 5. Cleanup: the CM6 RangeSet must not hold overlapping replaces — a
  //    replace strictly contained in another is dropped (the outer one, e.g.
  //    a table or display-math line, renders). Spans fully inside a replaced
  //    range are unrenderable and dropped too; a span merely crossing a
  //    replaced range survives (the mark continues across the hidden text).
  const big = replaces.map((r) => [r.from, r.to]);
  const inOtherReplace = (f, t) =>
    big.some(([s, e]) => f >= s && t <= e && (f > s || t < e));
  return {
    replaces: replaces.filter((r) => !inOtherReplace(r.from, r.to)),
    spans: spans.filter((sp) => !inOtherReplace(sp.from, sp.to)),
    lines: [...lineClasses.entries()]
      .sort((a, b) => a[0] - b[0])
      .map(([i, set]) => ({ from: lines.from(i), to: lines.to(i), className: [...set].join(" ") })),
    blocks,
  };
}

// -- tree walk ---------------------------------------------------------------
//
// One cursor drives the depth-first walk; `ctx` carries the accumulators and
// the walk state (list depth/kinds, task-item suppression, quote depth).
// Handlers that must visit children call descend(); block-shaped nodes
// (tables, fences) never descend — their source lines are replaced as a unit.

function descend(cur, ctx) {
  // Iterate the children on a fresh cursor at this node: visiting a child may
  // descend into it, which would otherwise strand the caller's cursor inside
  // the child's subtree and skip this node's remaining siblings.
  const c = cur.node.cursor();
  if (c.firstChild()) do { visit(c, ctx); } while (c.nextSibling());
}

// SyntaxNode does not expose its children; walk them with a local cursor.
function childrenOf(node) {
  const out = [];
  const cur = node.cursor();
  if (cur.firstChild()) do { out.push(cur.node); } while (cur.nextSibling());
  return out;
}

function visit(cur, ctx) {
  const node = cur.node;
  // The frontmatter's source is replaced as a unit (section 4); the
  // grammar misparses it (thematic breaks / a setext underline) and those
  // misparse nodes are skipped here. The test is `node.to <=`, NOT
  // `node.from <`: the walk starts at the Document root, which begins at
  // 0 — a from-based test skips the root itself, aborts the whole walk,
  // and a note with frontmatter gets NO structural decorations (headings,
  // emphasis, lists, fences, images all raw source; only the pure
  // scanners — wikilinks, math — render). Found in the second human GUI
  // pass: frontmatter notes rendered as raw markdown.
  if (ctx.fmRange && node.to <= ctx.fmRange[1]) return;

  switch (node.name) {
    case "FencedCode": return fence(node, ctx);
    case "CodeBlock": return indentedCode(node, ctx);
    case "Table": return table(node, ctx);
    case "HorizontalRule": return hr(node, ctx);
    case "QuoteMark": return quoteMark(node, ctx);
    case "ListMark": return listMark(node, ctx);
    case "StrongEmphasis":
    case "Emphasis":
    case "Strikethrough": return emphasis(node, ctx);
    case "InlineCode": return inlineCode(node, ctx);
    case "Link": return link(node, ctx);
    case "Autolink": return autolink(node, ctx);
    case "URL": return bareUrl(cur, ctx);
    case "Image": return image(node, ctx);
  }
  if (node.name.startsWith("ATXHeading")) return atxHeading(node, ctx);
  if (node.name === "SetextHeading1" || node.name === "SetextHeading2")
    return setextHeading(node, ctx);
  if (node.name === "Blockquote") return blockquote(cur, ctx);
  if (node.name === "ListItem") return listItem(cur, ctx);
  if (node.name === "BulletList" || node.name === "OrderedList") return list(cur, ctx);
  descend(cur, ctx);
}

function fence(node, ctx) {
  const { text, lines, addReplace, addLineClass, blocks } = ctx;
  const firstNl = text.indexOf("\n", node.from);
  if (firstNl === -1) return; // malformed one-liner: leave literal
  let info = "";
  let closeMark = null;
  for (const c of childrenOf(node)) {
    if (c.name === "CodeInfo") info = text.slice(c.from, c.to).trim();
    else if (c.name === "CodeMark") closeMark = c; // last CodeMark = closing fence
  }
  if (!closeMark) return;
  const openIdx = lines.indexAt(node.from);
  const closeIdx = lines.indexAt(closeMark.from);
  addReplace(node.from, firstNl + 1); // opening line, incl. its newline
  addReplace(closeMark.from, closeMark.to); // closing fence text only
  for (let i = openIdx + 1; i <= closeIdx; i++) {
    addLineClass(i, "tandem-code-line");
    if (i === closeIdx) addLineClass(i, "tandem-code-end");
  }
  // Language-tag row above the first code line (or above the closing line
  // for an empty block).
  blocks.push({
    at: lines.from(Math.min(openIdx + 1, closeIdx)),
    side: 0,
    widget: { kind: "codelangtag", info },
  });
}

function indentedCode(node, ctx) {
  const { lines, addLineClass } = ctx;
  const a = lines.indexAt(node.from);
  const b = lines.indexAt(node.to - 1);
  for (let i = a; i <= b; i++) {
    addLineClass(i, "tandem-codeblock");
    if (i === a) addLineClass(i, "tandem-codeblock-first");
    if (i === b) addLineClass(i, "tandem-codeblock-last");
  }
}

function table(node, ctx) {
  const { text, lines, addReplace, blocks } = ctx;
  const header = [];
  const rows = [];
  for (const c of childrenOf(node)) {
    if (c.name !== "TableHeader" && c.name !== "TableRow") continue;
    const cells = childrenOf(c)
      .filter((t) => t.name === "TableCell")
      .map((t) => text.slice(t.from, t.to));
    if (c.name === "TableHeader") header.push(...cells);
    else rows.push(cells);
  }
  const [bf, bt] = blockLineRange(lines, text, node.from, node.to);
  addReplace(bf, bt);
  blocks.push({
    at: bf,
    side: 0,
    widget: { kind: "table", header: header.length ? header : null, rows },
  });
}

function hr(node, ctx) {
  const { text, lines, addReplace, blocks } = ctx;
  const [bf, bt] = blockLineRange(lines, text, node.from, node.to);
  addReplace(bf, bt);
  blocks.push({ at: bf, side: 0, widget: { kind: "hr" } });
}

function atxHeading(node, ctx) {
  const { text, lines, addReplace, addLineClass } = ctx;
  const level = Number(node.name.slice(10));
  addLineClass(lines.indexAt(node.from), "tandem-h" + level);
  for (const c of childrenOf(node)) {
    if (c.name !== "HeaderMark") continue;
    let f = c.from;
    let t = c.to;
    if (f === node.from) {
      while (t < node.to && text[t] === " ") t++; // spaces after the opening marks
    } else {
      while (f > node.from && text[f - 1] === " ") f--; // before the closing marks
    }
    addReplace(f, t);
  }
}

function setextHeading(node, ctx) {
  const { lines, addReplace, addLineClass } = ctx;
  const level = node.name === "SetextHeading1" ? 1 : 2;
  addLineClass(lines.indexAt(node.from), "tandem-h" + level);
  for (const c of childrenOf(node)) {
    if (c.name !== "HeaderMark") continue;
    const [f, t] = lines.withBreak(lines.indexAt(c.from));
    addReplace(f, t); // whole underline line, incl. its newline
  }
}

function blockquote(cur, ctx) {
  const node = cur.node;
  const { lines, addLineClass } = ctx;
  if (ctx.inQuote === 0) {
    const a = lines.indexAt(node.from);
    const b = lines.indexAt(node.to - 1);
    for (let i = a; i <= b; i++) {
      addLineClass(i, "tandem-quote");
      if (i === a) addLineClass(i, "tandem-quote-first");
      if (i === b) addLineClass(i, "tandem-quote-last");
    }
  }
  ctx.inQuote++;
  descend(cur, ctx);
  ctx.inQuote--;
}

function quoteMark(node, ctx) {
  const { text, addReplace } = ctx;
  let t = node.to;
  while (t < text.length && text[t] === " ") t++;
  addReplace(node.from, t);
}

function list(cur, ctx) {
  ctx.listKind.push(cur.node.name === "OrderedList" ? "ordered" : "bullet");
  descend(cur, ctx);
  ctx.listKind.pop();
}

function listItem(cur, ctx) {
  const node = cur.node;
  const { text, lines, addReplace, addLineClass } = ctx;
  const mark = childrenOf(node).find((c) => c.name === "ListMark");
  const task = childrenOf(node).find((c) => c.name === "Task");
  if (mark) {
    const i = lines.indexAt(mark.from);
    if (lines.from(i) < mark.from)
      addReplace(lines.from(i), mark.from); // indent rendered by the depth class
    const depth = ctx.listKind.length - 1;
    if (depth > 0) addLineClass(i, "tandem-li-d" + Math.min(depth, 8));
    if (task) {
      // No bullet on a task item: the checkbox stands in for it.
      let t = mark.to;
      while (t < task.from && (text[t] === " " || text[t] === "\t")) t++;
      addReplace(mark.from, t);
      const tm = childrenOf(task).find((c) => c.name === "TaskMarker");
      if (tm) {
        // TaskMarker spans `[x]`/`[ ]`; the state char is the middle one.
        const checked = text[tm.from + 1] === "x" || text[tm.from + 1] === "X";
        addReplace(tm.from, tm.to, { kind: "checkbox", checked, markerFrom: tm.from });
        if (checked) addLineClass(i, "tandem-task-done");
      }
      ctx.inTaskItem++;
    }
  }
  descend(cur, ctx);
  if (task) ctx.inTaskItem--;
}

function listMark(node, ctx) {
  if (ctx.inTaskItem > 0) return;
  const { text, addReplace } = ctx;
  const ordered = ctx.listKind[ctx.listKind.length - 1] === "ordered";
  if (ordered)
    addReplace(node.from, node.to, {
      kind: "bullet",
      number: text.slice(node.from, node.to - 1), // strip the trailing . or )
    });
  else addReplace(node.from, node.to, { kind: "bullet" });
}

function emphasis(node, ctx) {
  const { addReplace, spans } = ctx;
  const cls =
    node.name === "StrongEmphasis"
      ? "tandem-strong"
      : node.name === "Emphasis"
        ? "tandem-em"
        : "tandem-strike";
  const marks = childrenOf(node).filter(
    (c) => c.name === "EmphasisMark" || c.name === "StrikethroughMark"
  );
  if (marks.length < 2) return;
  for (const m of marks) addReplace(m.from, m.to);
  spans.push({ from: marks[0].to, to: marks[marks.length - 1].from, className: cls });
}

function inlineCode(node, ctx) {
  const { addReplace, spans } = ctx;
  const marks = childrenOf(node).filter((c) => c.name === "CodeMark");
  if (marks.length < 2) return;
  for (const m of marks) addReplace(m.from, m.to);
  spans.push({ from: marks[0].to, to: marks[marks.length - 1].from, className: "tandem-code" });
}

function link(node, ctx) {
  const { text, addReplace, spans } = ctx;
  const url = childrenOf(node).find((c) => c.name === "URL");
  if (!url) return; // a reference-style [text] stays literal
  const marks = childrenOf(node).filter((c) => c.name === "LinkMark");
  if (marks.length < 2 || marks[0].to >= marks[1].from) return;
  addReplace(marks[0].from, marks[0].to); // opening [
  addReplace(marks[1].from, node.to); // closing ] + (url) hidden as one range
  spans.push({
    from: marks[0].to,
    to: marks[1].from,
    className: "tandem-link",
    dataHref: text.slice(url.from, url.to),
  });
}

function autolink(node, ctx) {
  const { text, addReplace, spans } = ctx;
  const url = childrenOf(node).find((c) => c.name === "URL");
  if (!url) return;
  for (const c of childrenOf(node))
    if (c.name === "LinkMark") addReplace(c.from, c.to);
  spans.push({
    from: url.from,
    to: url.to,
    className: "tandem-link",
    dataHref: text.slice(url.from, url.to),
  });
}

function bareUrl(cur, ctx) {
  const node = cur.node;
  const p = cur.parentType && cur.parentType.name;
  if (p === "Link" || p === "Image" || p === "Autolink") return; // handled by the parent
  const { text, spans } = ctx;
  spans.push({
    from: node.from,
    to: node.to,
    className: "tandem-link",
    dataHref: text.slice(node.from, node.to),
  });
}

function image(node, ctx) {
  const { text, lines, addReplace, blocks, notePath } = ctx;
  const url = childrenOf(node).find((c) => c.name === "URL");
  if (!url) return;
  const src = text.slice(url.from, url.to);
  const marks = childrenOf(node).filter((c) => c.name === "LinkMark");
  const alt = marks.length >= 2 ? text.slice(marks[0].to, marks[1].from) : "";
  const i = lines.indexAt(node.from);
  const lineText = text.slice(lines.from(i), lines.to(i)).trim();
  if (lineText === text.slice(node.from, node.to).trim() && lineText.length > 0) {
    // Whole-line image: a block figure (img or broken-image placeholder).
    addReplace(lines.from(i), lines.withBreak(i)[1]);
    blocks.push({ at: lines.from(i), side: 0, widget: { kind: "image", src, alt, notePath } });
  } else {
    // Mid-line image: the markup is hidden, a compact img shows inline.
    addReplace(node.from, node.to, { kind: "inlineimage", src, alt, notePath });
  }
}
