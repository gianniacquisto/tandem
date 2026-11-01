// The mermaid render path (spec: Editor, rendered feature list: mermaid).
//
// The fence is the whole interface: blocks.js plans a `mermaid` block widget
// for a ```mermaid fence, widgets.js mounts `div.tandem-mermaid`, and this
// module draws the SVG into it. Rendering is async and view-only: a failed
// parse leaves the raw source visible under `.tandem-mermaid-error`, the same
// contract display math has, and the buffer is never touched.
//
// mermaid is a ~5 MB bundle (the cost note measured 12.2.0 at 5,494,801 B
// minified), so it is a module-level lazy `import()`: esbuild still inlines
// it into the one committed editor.js, while the node test process (which
// never calls loadMermaid) never pays for it. No network: mermaid runs with
// `securityLevel: "strict"`, and this module adds no fetch/XHR path of its
// own.

const DAY = "day";
const NIGHT = "night";

// The page's palette (style.css' Ligne custom properties), with the day
// values as the fallbacks for a context that has no computed style (Node).
export function palette(doc = globalThis.document) {
  const styles =
    doc && typeof globalThis.getComputedStyle === "function"
      ? globalThis.getComputedStyle(doc.documentElement)
      : null;
  const read = (name, fallback) => {
    const value = styles?.getPropertyValue?.(name)?.trim();
    return value ? value : fallback;
  };
  return {
    paper: read("--paper", "#f1e9d7"),
    parchment: read("--parchment", "#e7deca"),
    sand: read("--sand", "#d6c9ae"),
    ink: read("--ink", "#211d18"),
    inkDim: read("--ink-dim", "#877d6b"),
    accent: read("--accent", "#6e7d6e"),
    accentDeep: read("--accent-deep", "#55604f"),
    ember: read("--ember", "#a9745c"),
  };
}

// Ligne's palette as mermaid's own theme variables, so a diagram is drawn in
// the app's colours rather than mermaid's default theme. `theme: "base"` is
// the empty theme these variables fill in.
export function themeVariables(doc = globalThis.document) {
  const p = palette(doc);
  return {
    background: p.paper,
    primaryColor: p.parchment,
    primaryTextColor: p.ink,
    primaryBorderColor: p.sand,
    secondaryColor: p.sand,
    tertiaryColor: p.parchment,
    lineColor: p.inkDim,
    textColor: p.ink,
    mainBkg: p.parchment,
    nodeBorder: p.sand,
    clusterBkg: p.paper,
    clusterBorder: p.sand,
    edgeLabelBackground: p.paper,
    titleColor: p.ink,
    secondaryBorderColor: p.accent,
    tertiaryBorderColor: p.accentDeep,
    noteBkgColor: p.parchment,
    noteBorderColor: p.ember,
    noteTextColor: p.ink,
    errorBkgColor: p.parchment,
    errorTextColor: p.ember,
  };
}

// The theme the page is in: the shell sets `data-theme` on <html> (day
// is the default, Ligne's day/night pair).
export function currentTheme(doc = globalThis.document) {
  const declared = doc?.documentElement?.getAttribute?.("data-theme");
  return declared === NIGHT ? NIGHT : DAY;
}

// -- lazy mermaid ---------------------------------------------------------------

let mermaidPromise = null;
let configuredTheme = null;

function configure(mermaid, theme, doc) {
  if (configuredTheme === theme) return;
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    theme: "base",
    themeVariables: themeVariables(doc),
  });
  configuredTheme = theme;
}

// The module, imported and configured once. A theme flip re-initializes with
// the new palette's variables (mermaid's config is module-global, so this is
// the way the same source is drawn in night colours).
export function loadMermaid(theme = currentTheme(), doc = globalThis.document) {
  if (!mermaidPromise) {
    mermaidPromise = import("mermaid").then((mod) => {
      const mermaid = mod.default ?? mod;
      configure(mermaid, theme, doc);
      return mermaid;
    });
  }
  return mermaidPromise.then((mermaid) => {
    configure(mermaid, theme, doc);
    return mermaid;
  });
}

let diagramId = 0;

async function renderWithMermaid(source, theme) {
  const mermaid = await loadMermaid(theme);
  const { svg } = await mermaid.render("tandem-mermaid-" + ++diagramId, source);
  return svg;
}

// -- render ------------------------------------------------------------------------

// Source plus theme → SVG, module-wide: a re-decoration (a caret move, a
// theme flip back and forth) reuses the drawn SVG instead of re-rendering
// the same diagram. Keyed by both, because the SVG carries the colours.
export const diagramCache = new Map();

/**
 * Draw `source` into `element`. Resolves to the SVG string, or null when the
 * render rejected (mermaid's parse error): the element then shows the raw
 * source as monospace ink under `.tandem-mermaid-error`, exactly as a KaTeX
 * parse error shows its own source. An injected `render` is the test seam;
 * `cache` is a Map keyed by theme + source.
 */
export async function renderDiagram(
  element,
  source,
  theme,
  { render = renderWithMermaid, cache = diagramCache } = {}
) {
  const key = theme + "\u0000" + source;
  let svg = cache.get(key);
  if (svg === undefined) {
    try {
      svg = await render(source, theme);
    } catch {
      element.classList.add("tandem-mermaid-error");
      element.textContent = source;
      return null;
    }
    cache.set(key, svg);
  }
  element.classList.remove("tandem-mermaid-error");
  element.innerHTML = svg;
  return svg;
}
