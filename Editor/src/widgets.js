// WidgetType implementations for the live-preview decorations.
//
// View-only: none of these widgets mutate the document. The raw markdown in
// the buffer is the source of truth (spec invariant 1); the single exception
// is the task checkbox, whose click dispatches a deliberate one-character
// transaction (spec: interactive task checkboxes).

import { WidgetType } from "@codemirror/view";
import katex from "katex";
import { postCheckboxToggled, postLinkOpened } from "./bridge.js";
import { resolveImageTarget } from "./pure/image.js";

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

// -- Wikilink -----------------------------------------------------------------

// Replaces the whole [[…]] range; shows the display text (alias | fragment |
// target). Click → link-opened with the raw target exactly as written (the
// Swift host strips fragment/alias per the wikilink grammar). Resolved links
// are dustyBlue; broken links are emberDeep; no underline (Ligne restraint).
export class WikilinkWidget extends WidgetType {
  constructor(target, display, broken) {
    super();
    this.target = target;
    this.display = display;
    this.broken = broken;
  }
  eq(other) {
    return (
      other instanceof WikilinkWidget &&
      other.target === this.target &&
      other.broken === this.broken
    );
  }
  toDOM() {
    return el(
      "span",
      this.broken ? "tandem-wikilink tandem-wikilink-broken" : "tandem-wikilink",
      this.display
    );
  }
  eventHandlers() {
    return {
      click(event) {
        event.preventDefault();
        event.stopPropagation();
        postLinkOpened(this.target);
        return true;
      },
    };
  }
}

// -- Task checkbox --------------------------------------------------------------

// Replaces the TaskMarker ([ ] / [x]) with a 14 pt checkbox (prototype:
// LigneCheckbox at size 14). Click = a one-character transaction flipping
// the char at markerFrom + 2; the doc change emits dirty, and the
// informational checkbox-toggled message goes out with the 1-based line.
//
// markerFrom is captured at decoration time (decorations recompute on every
// update, so it is always fresh) and is part of eq(): a reused DOM node must
// not keep a stale position in its click handler.
export class CheckboxWidget extends WidgetType {
  constructor(checked, markerFrom) {
    super();
    this.checked = checked;
    this.markerFrom = markerFrom;
  }
  eq(other) {
    return (
      other instanceof CheckboxWidget &&
      other.checked === this.checked &&
      other.markerFrom === this.markerFrom
    );
  }
  toDOM() {
    const box = el(
      "span",
      "tandem-checkbox" + (this.checked ? " tandem-checkbox-on" : "")
    );
    if (this.checked) {
      const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      svg.setAttribute("viewBox", "0 0 14 14");
      svg.setAttribute("width", "9");
      svg.setAttribute("height", "9");
      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.setAttribute("d", "M3 7.5 L6 10.5 L11 4");
      path.setAttribute("fill", "none");
      path.setAttribute("stroke", "var(--paper)");
      path.setAttribute("stroke-width", "2");
      path.setAttribute("stroke-linecap", "round");
      path.setAttribute("stroke-linejoin", "round");
      svg.appendChild(path);
      box.appendChild(svg);
    }
    return box;
  }
  eventHandlers() {
    return {
      mousedown(event) {
        // Keep the editor's focus (no blur/re-focus dance).
        event.preventDefault();
        return true;
      },
      click(event, view) {
        event.preventDefault();
        event.stopPropagation();
        toggleTask(view, this.markerFrom);
        return true;
      },
    };
  }
}

// Flips the state char inside the TaskMarker ("[ ]" <-> "[x]").
// markerFrom is the marker's opening "[", so the char is markerFrom + 1.
// The current state is read from the buffer (source of truth), not from
// the widget. Exported so the toggle transaction + payload are testable
// headlessly (the click handler itself is DOM-only, verified in the GUI
// smoke test).
export function toggleTask(view, markerFrom) {
  const state = view.state;
  const line = state.doc.lineAt(markerFrom);
  const pos = markerFrom + 1;
  if (pos < line.from || pos + 1 > line.to) return;
  const current = state.doc.sliceString(pos, pos + 1);
  if (current !== " " && current !== "x") return;
  const next = current === " " ? "x" : " ";
  view.dispatch({
    changes: { from: pos, to: pos + 1, insert: next },
    userEvent: "tandem.toggleTask",
    selection: { anchor: pos, head: pos },
  });
  postCheckboxToggled(line.number, next === "x");
}

// -- List bullets -----------------------------------------------------------------

// Replaces the ListMark: a 4 pt dustyBlue dot for unordered lists (prototype
// bullet), the written number for ordered (inkDim monospaced).
export class BulletWidget extends WidgetType {
  constructor(numberText) {
    super();
    this.numberText = numberText; // undefined for unordered
  }
  eq(other) {
    return other instanceof BulletWidget && other.numberText === this.numberText;
  }
  toDOM() {
    if (this.numberText === undefined) return el("span", "tandem-bullet");
    return el("span", "tandem-bullet-num", this.numberText);
  }
}

// -- Math (KaTeX) ------------------------------------------------------------------

// Render into a node; on a KaTeX ParseError leave the raw math visible (with
// its delimiters) as monospace text (spec: parse errors leave raw visible).
function renderMath(node, content, displayMode, delimiters) {
  try {
    katex.render(content, node, { throwOnError: true, displayMode, output: "html" });
  } catch {
    node.classList.add("tandem-math-error");
    node.textContent = delimiters + content + delimiters;
  }
}

export class MathWidget extends WidgetType {
  constructor(content) {
    super();
    this.content = content;
  }
  eq(other) {
    return other instanceof MathWidget && other.content === this.content;
  }
  toDOM() {
    const span = el("span", "tandem-math");
    renderMath(span, this.content, false, "$");
    return span;
  }
}

// Block widget replacing a display-math range ($$…$$ on its own lines).
export class DisplayMathWidget extends WidgetType {
  constructor(content) {
    super();
    this.content = content;
    this.block = true;
  }
  eq(other) {
    return other instanceof DisplayMathWidget && other.content === this.content;
  }
  toDOM() {
    const div = el("div", "tandem-display-math");
    renderMath(div, this.content, true, "$$");
    return div;
  }
}

// -- Images ---------------------------------------------------------------------------

// Block widget for a whole-line image. Local targets render as
// <img src="tandem:<vault path>"> (the Swift scheme handler serves the file
// under the vault root; a missing file 404s and the error handler swaps in
// the broken-image placeholder with alt text). Remote and escaping targets
// render the placeholder directly — never fetched (no-network invariant).
// Container-width cap is CSS (.tandem-img).
export class ImageWidget extends WidgetType {
  constructor(src, alt, notePath) {
    super();
    this.src = src;
    this.alt = alt;
    this.notePath = notePath;
    // Resolved once at decoration time (pure, no DOM): remote http(s)
    // targets are rewritten to the broken-image placeholder before render —
    // never fetched (no-network invariant); vault targets become
    // `tandem:<path>` (the Swift scheme handler serves the file; a missing
    // file 404s and the error handler swaps in the placeholder with alt).
    this.target = resolveImageTarget(src, notePath);
    this.block = true;
  }
  eq(other) {
    return (
      other instanceof ImageWidget &&
      other.src === this.src &&
      other.alt === this.alt &&
      other.notePath === this.notePath
    );
  }
  toDOM() {
    if (this.target.kind !== "vault" || this.target.path === "") {
      return brokenImage(this.alt, this.src, this.target.kind === "remote");
    }
    const img = document.createElement("img");
    img.className = "tandem-img";
    img.alt = this.alt;
    img.src = "tandem:" + this.target.path;
    img.addEventListener("error", () =>
      img.replaceWith(brokenImage(this.alt, this.src, false))
    );
    return img;
  }
}

// Inline image (an image mid-line): shows the alt text only — the target is
// never fetched and the ![]() syntax is view-hidden.
export class InlineImageWidget extends WidgetType {
  constructor(alt) {
    super();
    this.alt = alt;
  }
  eq(other) {
    return other instanceof InlineImageWidget && other.alt === this.alt;
  }
  toDOM() {
    return el("span", "tandem-inline-img", this.alt);
  }
}

// Broken-image placeholder (prototype §10): photo glyph + "missing · <alt or
// ref>" (or "remote images are never fetched") in emberDeep 11.5 medium, the
// written target beneath in 10.5 monospace inkDim; 1 pt dashed ember border,
// 9 corners, max width 300, padding 10.
function brokenImage(alt, ref, remote) {
  const wrap = el("div", "tandem-img-broken");
  const head = el("div", "tandem-img-broken-head");
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", "13");
  svg.setAttribute("height", "13");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.8");
  const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  rect.setAttribute("x", "2.5");
  rect.setAttribute("y", "4.5");
  rect.setAttribute("width", "19");
  rect.setAttribute("height", "15");
  rect.setAttribute("rx", "2.5");
  const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  circle.setAttribute("cx", "8.5");
  circle.setAttribute("cy", "10");
  circle.setAttribute("r", "1.75");
  const peak = document.createElementNS("http://www.w3.org/2000/svg", "path");
  peak.setAttribute("d", "M4.5 17.5 L10 12.5 L14 16 L17.5 13 L20 15.5");
  svg.appendChild(rect);
  svg.appendChild(circle);
  svg.appendChild(peak);
  const label = el("span", "tandem-img-broken-label");
  label.textContent = remote
    ? "remote images are never fetched"
    : "missing" + (alt || ref ? " · " + (alt || ref) : "");
  head.appendChild(svg);
  head.appendChild(label);
  wrap.appendChild(head);
  if (ref) wrap.appendChild(el("div", "tandem-img-ref", ref));
  return wrap;
}

// -- Frontmatter --------------------------------------------------------------------------

// Block widget replacing the leading frontmatter block: the preview strips it
// (never parsed, never shown); the source stays in the buffer.
export class FrontmatterWidget extends WidgetType {
  constructor() {
    super();
    this.block = true;
  }
  eq() {
    return true;
  }
  toDOM() {
    return el("div", "tandem-frontmatter", "— frontmatter —");
  }
}

// -- Tables ---------------------------------------------------------------------------------

// Block widget replacing a whole GFM table with a rendered <table>
// (prototype: cells 12.5 pt, header semibold, 1 pt ink under the header,
// 1 pt sand between rows). Cell text stays in the buffer; the widget is
// non-editable in v1 (spec: tables render, editing is by source).
export class TableWidget extends WidgetType {
  constructor(header, rows) {
    super();
    this.header = header; // string[] or null
    this.rows = rows; // string[][]
    this.block = true;
  }
  eq(other) {
    if (!(other instanceof TableWidget)) return false;
    const same = (a, b) =>
      a === b ||
      (a !== null && b !== null && a.length === b.length && a.every((c, i) => c === b[i]));
    if (!same(this.header, other.header)) return false;
    if (this.rows.length !== other.rows.length) return false;
    for (let i = 0; i < this.rows.length; i++) {
      const r = this.rows[i];
      const o = other.rows[i];
      if (r.length !== o.length || r.some((c, j) => c !== o[j])) return false;
    }
    return true;
  }
  toDOM() {
    const table = el("table", "tandem-table");
    const appendRow = (cells, isHeader) => {
      const tr = document.createElement("tr");
      cells.forEach((text) => {
        const cell = document.createElement(isHeader ? "th" : "td");
        cell.textContent = text;
        tr.appendChild(cell);
      });
      table.appendChild(tr);
    };
    if (this.header) appendRow(this.header, true);
    this.rows.forEach((r) => appendRow(r, false));
    return table;
  }
}

// -- Fenced code --------------------------------------------------------------------------------

// Block widget on the opening fence line: the language-tag row (right-aligned
// 10.5 pt monospaced inkDim, prototype §10). info is the info string as
// written ("" when none).
export class CodeTagWidget extends WidgetType {
  constructor(info) {
    super();
    this.info = info;
    this.block = true;
  }
  eq(other) {
    return other instanceof CodeTagWidget && other.info === this.info;
  }
  toDOM() {
    return el("div", "tandem-code-tag", this.info);
  }
}

// -- Horizontal rule -----------------------------------------------------------------

// Block widget replacing the hr line: a 1 pt sand hairline (Ligne). The
// `---` source stays in the buffer; the widget is view-only.
export class HrWidget extends WidgetType {
  constructor() {
    super();
    this.block = true;
  }
  eq() {
    return true;
  }
  toDOM() {
    return el("div", "tandem-hr");
  }
}

// -- Soft per-note ceiling -----------------------------------------------------------------

// Block widget pinned above the first line when the document passes the
// soft ceiling (~1 MB, spec: Scale): a quiet ember warning row. View-only —
// the document is never touched by the warning.
export class CeilingWarningWidget extends WidgetType {
  constructor(message) {
    super();
    this.message = message;
    this.block = true;
  }
  eq(other) {
    return (
      other instanceof CeilingWarningWidget && other.message === this.message
    );
  }
  toDOM() {
    return el("div", "tandem-ceiling-warning", this.message);
  }
}
