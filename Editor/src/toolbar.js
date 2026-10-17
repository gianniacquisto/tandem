// The formatting toolbar — the quiet row above the note pane.
//
// Rendered by the page itself (toolbar.js builds it and inserts it before
// the editor container), NOT by SwiftUI: the toolbar travels with the
// editor bundle and each button is a pure transaction through main.js'
// `specFor`. The font-size control sets the `--tandem-editor-font-size`
// custom property on the editor root (main.js owns that, plus the bridge
// post) and only reports its label here.
//
// The register is Ligne (see style.css section 2): a hairline bottom
// border, paper/parchment surfaces, ink-dim line-drawn glyphs, no filled
// or saturated buttons — the note typography itself is untouched. The
// toolbar's own chrome is a fixed small size and deliberately does NOT
// use `--tandem-editor-font-size`, so it never scales with the note font.

const SVG_NS = "http://www.w3.org/2000/svg";

// One line-drawn glyph per action, 16×16, stroked in currentColor. Kept
// as inner-markup strings so the SVG helper below owns the shared
// attributes (viewBox, stroke width, caps).
const ICONS = {
  bold:
    '<path d="M5 3.5h3.4a2.3 2.3 0 0 1 0 4.6H5z"/>' +
    '<path d="M5 8.1h3.9a2.4 2.4 0 0 1 0 4.8H5z"/>',
  italic: '<path d="M8 3.5h4M4 12.5h4M9.6 3.5 6.4 12.5"/>',
  strikethrough:
    '<path d="M3.5 8h9"/>' +
    '<path d="M10.5 5.2c-.6-.9-1.7-1.4-2.9-1.4-1.6 0-2.7.7-2.7 1.9"/>' +
    '<path d="M5.5 10.8c.6.9 1.7 1.4 2.9 1.4 1.6 0 2.7-.7 2.7-1.9"/>',
  "inline-code": '<path d="M6 4 3 8l3 4M10 4l3 4-3 4"/>',
  link:
    '<path d="M6.5 9.5 9.5 6.5"/>' +
    '<path d="M7.4 4.9 8.9 3.4a2.5 2.5 0 0 1 3.5 3.5l-1.5 1.5"/>' +
    '<path d="M8.6 11.1 7.1 12.6a2.5 2.5 0 0 1-3.5-3.5l1.5-1.5"/>',
  heading: '<path d="M4 3.5v9M12 3.5v9M4 8h8"/>',
  "bullet-list":
    '<path d="M6.5 4h6.5M6.5 8h6.5M6.5 12h6.5"/>' +
    '<circle cx="3" cy="4" r="1"/>' +
    '<circle cx="3" cy="8" r="1"/>' +
    '<circle cx="3" cy="12" r="1"/>',
  "numbered-list":
    '<path d="M6.5 4h6.5M6.5 8h6.5M6.5 12h6.5"/>' +
    '<path d="M2 3.2 3 2.6V6M1.9 9.9c.3-.4.7-.6 1.1-.6.6 0 1 .4 1 .9 0 1-2 1.1-2 2.4h2"/>',
  "task-list":
    '<path d="M6.5 4h6.5M6.5 8h6.5M6.5 12h6.5"/>' +
    '<rect x="1.6" y="2.6" width="3" height="3" rx="1"/>' +
    '<path d="M2.3 4.1l.6.7 1.3-1.5"/>' +
    '<rect x="1.6" y="10.6" width="3" height="3" rx="1"/>',
  quote: '<path d="M4 3.5v9"/><path d="M7 5h6M7 8h6M7 11h4"/>',
  "code-block":
    '<rect x="2.5" y="3.5" width="11" height="9" rx="1.5"/>' +
    '<path d="M6.6 6.6 5.2 8l1.4 1.4M9.4 6.6 10.8 8l-1.4 1.4"/>',
};

// The bar's left-to-right order: inline actions, then the block actions.
// `separator` entries become a hairline rule.
const FORMAT_BUTTONS = [
  { kind: "bold", title: "Bold" },
  { kind: "italic", title: "Italic" },
  { kind: "strikethrough", title: "Strikethrough" },
  { kind: "inline-code", title: "Inline code" },
  { kind: "link", title: "Link" },
  { kind: "heading", title: "Heading (cycles H1 → H2 → H3 → body)" },
  { separator: true },
  { kind: "bullet-list", title: "Bullet list" },
  { kind: "numbered-list", title: "Numbered list" },
  { kind: "task-list", title: "Task list" },
  { kind: "quote", title: "Quote" },
  { kind: "code-block", title: "Code block" },
];

const formatSize = (px) =>
  Number.isInteger(px) ? String(px) : px.toFixed(1);

function svgIcon(doc, inner) {
  const svg = doc.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.innerHTML = inner;
  return svg;
}

function button(doc, className, title) {
  const el = doc.createElement("button");
  el.type = "button";
  el.className = className;
  el.title = title;
  return el;
}

/**
 * Build the toolbar and insert it before the editor container.
 *
 * handlers:
 *   onFormat(kind)  — dispatch the formatting action (main.js owns it)
 *   onFontSize(action) — "increase" | "decrease" | "reset" (main.js owns
 *                        the computation, the CSS property, and the post)
 * initialFontSize — the readout's starting value (the persisted size)
 *
 * Returns { element, setFontSize(px) } so main.js can re-point the readout
 * when the Swift side sends the stored size on `loaded`.
 */
export function mountToolbar(editorRoot, { onFormat, onFontSize } = {}, initialFontSize = 13.5) {
  const doc = editorRoot.ownerDocument;
  const bar = doc.createElement("div");
  bar.id = "tandem-toolbar";
  bar.setAttribute("role", "toolbar");

  // Keep the editor's focus (and the selection an action wraps) when a
  // button is pressed: preventDefault on mousedown stops the button from
  // taking focus; the click still fires.
  bar.addEventListener("mousedown", (event) => event.preventDefault());

  for (const item of FORMAT_BUTTONS) {
    if (item.separator) {
      const sep = doc.createElement("span");
      sep.className = "tandem-sep";
      bar.appendChild(sep);
      continue;
    }
    const btn = button(doc, "tandem-tool", item.title);
    btn.setAttribute("aria-label", item.title);
    btn.appendChild(svgIcon(doc, ICONS[item.kind]));
    btn.addEventListener("click", () => onFormat(item.kind));
    bar.appendChild(btn);
  }

  // The font-size control, pushed to the right: A− | size (reset) | A+.
  const group = doc.createElement("div");
  group.className = "tandem-fontsize";
  const dec = button(doc, "tandem-tool tandem-font-btn", "Decrease font size");
  dec.textContent = "A−";
  const readout = button(doc, "tandem-fontsize-readout", "");
  const inc = button(doc, "tandem-tool tandem-font-btn", "Increase font size");
  inc.textContent = "A+";
  dec.addEventListener("click", () => onFontSize("decrease"));
  inc.addEventListener("click", () => onFontSize("increase"));
  readout.addEventListener("click", () => onFontSize("reset"));
  group.append(dec, readout, inc);
  bar.appendChild(group);

  const setFontSize = (px) => {
    readout.textContent = formatSize(px);
    readout.title = `Font size ${formatSize(px)}px — click to reset`;
    readout.setAttribute("aria-label", `Font size ${formatSize(px)} pixels, click to reset`);
  };
  setFontSize(initialFontSize);

  if (editorRoot.parentNode) editorRoot.parentNode.insertBefore(bar, editorRoot);
  return { element: bar, setFontSize };
}
