// The CM6 entry for the Tandem bikeshed editor (ticket 07).
//
// Assembles the inline-live-preview editor from the existing parts:
//   - blocks.js  — the plan builder (pure: planPreview)
//   - widgets.js — the CM6 widgets (wikilink, checkbox, math, image, …)
//   - theme.js   — the Ligne day/night theme
//   - bridge.js  — JS → Swift messages
//   - toolbar.js — the page-rendered formatting bar + font-size control
//
// Architecture: everything headless-testable lives above the
// `typeof document` guard at the bottom of this file.
//
//   - `editorExtensions()` — the full Extension set: vault index + note
//     path, the live-preview plan field, the GFM markdown language with the
//     curated code languages, and the Ligne theme.
//   - `livePreviewState` — a StateField<DecorationSet> that recomputes
//     planPreview from syntaxTree(state) + vault + note path on every
//     document change and every setVaultIndex/setDocument effect. Block
//     decorations (tables, display math, images, frontmatter, code
//     language tag, hr) come from this StateField — CM6 forbids block
//     decorations from a ViewPlugin's decorations facet.
//   - `vaultNotesState` / `notePathState` — where the plan sees the bridge
//     file set and the open note's path; facets seed the initial values,
//     effects (from the page globals) update them.
//   - `createEditorState({doc, vaultNotes, notePath})` — a plain
//     EditorState (no DOM): the seam the headless tests drive.
//   - Bridge decision logic: `dirtyTextForUpdate` (what `dirty` carries)
//     and the page-global transaction specs (setDocument/insertText/
//     revealLine/setVaultIndex) — all testable on plain states.
//   - Formatting actions: `wrapSelectionSpec` / `toggleLinePrefixSpec` /
//     `cycleHeadingSpec` / `fencedBlockSpec` (the toolbar's edits) and the
//     font-size helpers — also pure, testable on plain states.
//
// Invariants (spec): raw markdown is the source of truth (decorations are
// view-only); line height comes from line CSS classes, never from token
// visibility. The JS has no file or network access — everything crosses
// the bridge.

import { Facet, StateEffect, StateField, EditorState } from "@codemirror/state";
import { Decoration, EditorView } from "@codemirror/view";
import { syntaxTree } from "@codemirror/language";
import { basicSetup } from "codemirror";
import {
  markdown,
  markdownLanguage,
  commonmarkLanguage,
} from "@codemirror/lang-markdown";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { python } from "@codemirror/lang-python";
import { sql } from "@codemirror/lang-sql";

import { planPreview } from "./blocks.js";
import {
  WikilinkWidget,
  CheckboxWidget,
  BulletWidget,
  MathWidget,
  DisplayMathWidget,
  ImageWidget,
  InlineImageWidget,
  FrontmatterWidget,
  TableWidget,
  CodeTagWidget,
  HrWidget,
  CeilingWarningWidget,
  toggleTask,
} from "./widgets.js";
import { ligneTheme } from "./theme.js";
import { postDirty, postLinkOpened, postLoaded, postFontSize } from "./bridge.js";
import { mountToolbar } from "./toolbar.js";

// -- Fenced code: the curated highlighting set ------------------------------------
//
// The language packages pinned in package.json, plus markdown's own
// languages. Unknown language → no nested parser → plain monospace, no
// highlighting (spec: Rendered feature list). The function form of
// `codeLanguages` (info → Language | null) is used on purpose: the
// array form matches through LanguageDescription.matchLanguageName,
// which in the pinned lang-markdown 6.5.2 requires an `alias` array on
// every entry — bare LanguageSupports (css(), html(), …) don't carry
// one and matching throws.

const jsSupport = javascript({ jsx: true });
// TypeScript via the SAME pinned @codemirror/lang-javascript (its `typescript`
// flag) — no new dependency. `ts` gets the plain TS dialect, `tsx` the
// JSX+TS dialect; `js`/`jsx` stay exactly as before.
const tsSupport = javascript({ typescript: true });
const tsxSupport = javascript({ jsx: true, typescript: true });

const CURATED_CODE_LANGUAGES = new Map([
  ["css", css().language],
  ["html", html().language],
  ["xhtml", html().language],
  ["svg", html().language],
  ["javascript", jsSupport.language],
  ["js", jsSupport.language],
  ["jsx", jsSupport.language],
  ["typescript", tsSupport.language],
  ["ts", tsSupport.language],
  ["tsx", tsxSupport.language],
  ["json", json().language],
  ["python", python().language],
  ["py", python().language],
  ["sql", sql().language],
  ["markdown", markdownLanguage],
  ["md", markdownLanguage],
  ["commonmark", commonmarkLanguage],
]);

/** The nested parser for a fenced code block's info string, or null. */
export function codeLanguageFor(info) {
  return CURATED_CODE_LANGUAGES.get(String(info ?? "").trim().toLowerCase()) ?? null;
}

// -- Vault index + note path (bridge: setVaultIndex / setDocument) ------------------
//
// A facet seeds the initial value (createEditorState); a StateField holds
// the live value, updated by an effect from the page globals. The plan
// field reads the StateField, so a re-index re-resolves wikilinks without
// touching the document (derived state — no stored link index).

export const setVaultIndexEffect = StateEffect.define(["string[]"]);
export const setNotePathEffect = StateEffect.define(["string"]);

export const vaultNotesFacet = Facet.define({
  combine: (v) => (v.length ? v[v.length - 1] : []),
});

export const vaultNotesState = StateField.define({
  // The second state.field arg is the require flag, not a fallback:
  // an unseeded facet is absent, and the empty vault is the default.
  create: (state) => state.field(vaultNotesFacet, false) ?? [],
  update: (value, tr) => {
    for (const e of tr.effects) if (e.is(setVaultIndexEffect)) value = e.value;
    return value;
  },
});

export const notePathFacet = Facet.define({
  combine: (v) => (v.length ? v[v.length - 1] : ""),
});

export const notePathState = StateField.define({
  create: (state) => state.field(notePathFacet, false) ?? "",
  update: (value, tr) => {
    for (const e of tr.effects) if (e.is(setNotePathEffect)) value = e.value;
    return value;
  },
});

// -- Soft per-note ceiling (spec: Scale) -------------------------------------------

// ~1 MB, where decoration recompute and IME composition deserve profiling:
// past this a visible warning sits above the first line.
export const NOTE_CEILING = 1024 * 1024;
export const CEILING_WARNING =
  "This note is over 1 MB — the editor may feel slow.";

// -- The live-preview plan field -----------------------------------------------------

function makeWidget(d) {
  switch (d.kind) {
    case "wikilink":
      return new WikilinkWidget(d.target, d.display, d.broken);
    case "checkbox":
      return new CheckboxWidget(d.checked, d.markerFrom);
    case "bullet":
      return new BulletWidget(d.number);
    case "math":
      return new MathWidget(d.content);
    case "displaymath":
      return new DisplayMathWidget(d.content);
    case "image":
      return new ImageWidget(d.src, d.alt, d.notePath);
    case "inlineimage":
      return new InlineImageWidget(d.alt);
    case "frontmatter":
      return new FrontmatterWidget();
    case "table":
      return new TableWidget(d.header, d.rows);
    case "codelangtag":
      return new CodeTagWidget(d.info);
    case "hr":
      return new HrWidget();
    default:
      throw new RangeError("unknown plan widget kind: " + d.kind);
  }
}

// The plan → DecorationSet mapping: replaces (marker hides, with or
// without a widget), marks (spans; links carry their href as
// data-href for the click interception), line classes, and block widgets.
function previewDecorations(state) {
  const text = state.doc.toString();
  const plan = planPreview({
    text,
    tree: syntaxTree(state),
    notes: state.field(vaultNotesState),
    notePath: state.field(notePathState),
  });

  const ranges = [];
  for (const r of plan.replaces) {
    if (r.to <= r.from) continue;
    ranges.push(
      r.widget
        ? Decoration.replace({ widget: makeWidget(r.widget) }).range(r.from, r.to)
        : Decoration.replace({}).range(r.from, r.to)
    );
  }
  for (const s of plan.spans) {
    if (s.to <= s.from) continue;
    ranges.push(
      Decoration.mark(
        s.dataHref !== undefined
          ? { class: s.className, attributes: { "data-href": s.dataHref } }
          : { class: s.className }
      ).range(s.from, s.to)
    );
  }
  for (const l of plan.lines) {
    ranges.push(Decoration.line({ class: l.className }).range(l.from));
  }
  for (const b of plan.blocks) {
    ranges.push(
      Decoration.widget({
        widget: makeWidget(b.widget),
        side: b.side,
        block: true,
      }).range(b.at)
    );
  }
  if (text.length > NOTE_CEILING) {
    ranges.push(
      Decoration.widget({
        widget: new CeilingWarningWidget(CEILING_WARNING),
        side: 0,
        block: true,
      }).range(0)
    );
  }

  // The set must be ordered by (from, startSide): at a fenced code
  // block's start the line class and the block widgets share a
  // position, and their `side` values order them — pass sort=true so
  // the library applies that ordering.
  return Decoration.set(ranges, true);
}

export const livePreviewState = StateField.define({
  create: (state) => previewDecorations(state),
  update: (value, tr) => {
    const reindexed = tr.effects.some(
      (e) => e.is(setVaultIndexEffect) || e.is(setNotePathEffect)
    );
    return tr.docChanged || reindexed ? previewDecorations(tr.state) : value;
  },
  provide: (f) => [EditorView.decorations.from(f)],
});

// -- The extension set + headless state factory --------------------------------------

export function editorExtensions() {
  return [
    vaultNotesState,
    notePathState,
    livePreviewState,
    markdown({ base: markdownLanguage, codeLanguages: codeLanguageFor }),
    // Soft-wrap long lines: the pane never scrolls sideways.
    EditorView.lineWrapping,
    ...ligneTheme,
    basicSetup,
  ];
}

/**
 * A plain EditorState with the full editor configuration — no DOM.
 * The seam the headless tests drive; the browser glue builds the same
 * state and then wraps it in an EditorView.
 *
 * `viewExtensions` are the view-level extensions (the update listener and
 * the link-click handler). They MUST be part of the STATE, not passed in
 * the EditorView config's `extensions`: when a `state` is supplied, the
 * EditorView constructor ignores config.extensions and reads both the
 * `updateListener` facet and the `domEventHandlers` plugin from
 * `this.state` — so a listener handed in the config array never fires.
 */
export function createEditorState({
  doc = "",
  vaultNotes = [],
  notePath = "",
  viewExtensions = [],
} = {}) {
  return EditorState.create({
    doc,
    extensions: [
      ...editorExtensions(),
      ...(vaultNotes.length ? [vaultNotesFacet.of(vaultNotes)] : []),
      ...(notePath ? [notePathFacet.of(notePath)] : []),
      ...viewExtensions,
    ],
  });
}

// -- Bridge: JS → Swift decision logic (headless) ------------------------------------

// Marks a document change as the bridge's own setDocument — it replaces
// the document (open a note, adopt an external change, "Take theirs"),
// resets the dirty state, and emits nothing.
export const setDocumentAnnotation = StateEffect.define();

/**
 * The full raw text to emit as `dirty` for a batch of transactions, or
 * null when nothing is emitted. Any document change counts — typing,
 * checkbox toggles, insertText — except setDocument itself.
 */
export function dirtyTextForUpdate(transactions, state) {
  const changed = transactions.some(
    (tr) => tr.docChanged && !tr.annotation(setDocumentAnnotation)
  );
  return changed ? state.doc.toString() : null;
}

// The webview never navigates: every link in the preview is a span or a
// widget (WikilinkWidget spans carry the raw target as data-target — the
// display text may be an alias; the .tandem-link marks for markdown links,
// autolinks, and bare URLs carry data-href), never an <a>. The clicks are
// intercepted at the view level (editorViewExtensions) because the pinned
// @codemirror/view's WidgetType has no eventHandlers() — see the TRAP note
// in widgets.js. This is the pure decision the DOM glue applies to a
// click's target: the target to post as link-opened, or null when the click
// is not on a link.
export function linkOpenTargetFrom(target) {
  const closest = (sel) =>
    target && typeof target.closest === "function"
      ? target.closest(sel)
      : null;
  // Wikilink widget first: its data-target is the raw target as written
  // (the Swift host strips fragment/alias per the wikilink grammar).
  const wikilink = closest(".tandem-wikilink");
  if (wikilink) {
    const t =
      typeof wikilink.getAttribute === "function"
        ? wikilink.getAttribute("data-target")
        : null;
    if (typeof t === "string" && t !== "") return t;
  }
  const mark = closest(".tandem-link");
  const href =
    mark && typeof mark.getAttribute === "function"
      ? mark.getAttribute("data-href")
      : null;
  return typeof href === "string" && href !== "" ? href : null;
}

// The checkbox decision, same shape: the TaskMarker's buffer position for a
// click on a .tandem-checkbox widget (carried as data-marker-from, fresh at
// (re)draw — eq() includes it), or null when the click is not on a task
// checkbox.
export function checkboxToggleFrom(target) {
  const box =
    target && typeof target.closest === "function"
      ? target.closest(".tandem-checkbox")
      : null;
  const marker =
    box && typeof box.getAttribute === "function"
      ? box.getAttribute("data-marker-from")
      : null;
  // A buffer position is a non-negative integer; the attribute is only ever
  // written by toDOM as String(markerFrom). A strict digits-only match also
  // rejects the coercions Number("") === 0 and Number(" 2") === 2.
  if (typeof marker !== "string" || !/^\d+$/.test(marker)) return null;
  return Number(marker);
}

// -- Bridge: Swift → JS page globals (transaction specs, headless) -------------------
//
// Each returns a TransactionSpec for the current state; the guarded layer
// at the bottom dispatches them from the window globals. Testing a spec
// on a plain state therefore exercises the whole effect.

/** setDocument(markdown, notePath): replace the doc, reset dirty, emit nothing. */
export function setDocumentSpec(state, markdownText, notePath) {
  return {
    changes: { from: 0, to: state.doc.length, insert: markdownText },
    effects: [setNotePathEffect.of(notePath)],
    annotations: [setDocumentAnnotation.of(true)],
    selection: { anchor: 0 },
  };
}

/** insertText(text): insert at the caret. */
export function insertTextSpec(state, text) {
  const head = state.selection.main.head;
  return {
    changes: { from: head, insert: text },
    selection: { anchor: head + text.length },
  };
}

/** revealLine(line, 1-based): caret at the line start + scroll into view. */
export function revealLineSpec(state, line) {
  const n = state.doc.lines;
  const i = Math.max(1, Math.min(line, n));
  const from = state.doc.line(i).from;
  return {
    selection: { anchor: from },
    effects: [EditorView.scrollIntoView(from, { y: "start" })],
  };
}

/** setVaultIndex([{path}]): the current vault file set (notes only). */
export function setVaultIndexSpec(notes) {
  return { effects: [setVaultIndexEffect.of(notes)] };
}

// -- Formatting actions (toolbar; pure specs, headless) -------------------------------
//
// The toolbar (src/toolbar.js) is page chrome over the editor: each button
// dispatches one of these ordinary transactions, so autosave/conflict
// machinery is unchanged (a formatting edit is a document change like any
// other). The markdown stays the source of truth — the actions read the
// current buffer, never a rendered representation.

// The word placed between the markers when a wrapping action runs with an
// empty selection; it is selected so typing replaces it.
export const WRAP_PLACEHOLDER = "text";

/**
 * Wrap the current selection in `before`/`after`. With an empty selection
 * the placeholder word is inserted (and selected) between the markers.
 * The result selects the (wrapped) text, so the change stays reversible
 * and visible.
 */
export function wrapSelectionSpec(state, before, after = before) {
  const { from, to } = state.selection.main;
  if (from === to) {
    const insert = before + WRAP_PLACEHOLDER + after;
    const start = from + before.length;
    return {
      changes: { from, to, insert },
      selection: { anchor: start, head: start + WRAP_PLACEHOLDER.length },
      userEvent: "tandem.format.wrap",
    };
  }
  const changes = state.changes([
    { from, insert: before },
    { from: to, insert: after },
  ]);
  return {
    changes,
    // Keep the selection over the original text, now shifted by the
    // opening marker (and past it by the closing marker for head).
    selection: {
      anchor: changes.mapPos(from, -1),
      head: changes.mapPos(to, 1),
    },
    userEvent: "tandem.format.wrap",
  };
}

// The lines a selection "touches": the selection's start line through its
// end line, except that a selection ending exactly at a line boundary does
// not pull in the line after it.
function touchedLines(state) {
  const { from, to } = state.selection.main;
  const startLine = state.doc.lineAt(from);
  const endPos = to > from && to === state.doc.lineAt(to).from ? to - 1 : to;
  const endLine = state.doc.lineAt(endPos).number;
  const lines = [];
  for (let n = startLine.number; n <= endLine; n++) lines.push(state.doc.line(n));
  return lines;
}

const leadingSpaces = (text) => /^\s*/.exec(text)[0].length;

/**
 * Toggle a line `prefix` over every line the selection touches. If every
 * line already carries it (ignoring leading indentation) the prefix is
 * removed from each; otherwise it is added to the lines that lack it,
 * leaving indentation intact.
 */
export function toggleLinePrefixSpec(state, prefix) {
  const lines = touchedLines(state);
  const has = (line) => line.text.startsWith(prefix, leadingSpaces(line.text));
  const removing = lines.every(has);
  const changes = [];
  for (const line of lines) {
    const at = line.from + leadingSpaces(line.text);
    if (removing) {
      changes.push({ from: at, to: at + prefix.length, insert: "" });
    } else if (!has(line)) {
      changes.push({ from: at, insert: prefix });
    }
  }
  return lineChangeSpec(state, changes, "tandem.format.togglePrefix");
}

/**
 * Cycle the heading level of every touched line: body → H1 → H2 → H3 →
 * body. Any existing leading `#`s are replaced with the next level's, the
 * indentation preserved.
 */
export function cycleHeadingSpec(state) {
  const changes = [];
  for (const line of touchedLines(state)) {
    const indented = leadingSpaces(line.text);
    const m = /^(\s*)(#{1,6})(?: |$)/.exec(line.text);
    if (!m) {
      changes.push({ from: line.from + indented, insert: "# " });
      continue;
    }
    const at = line.from + m[1].length;
    const level = m[2].length;
    const next = level >= 3 ? 0 : level + 1;
    // Drop the existing hashes and their trailing space (when present).
    const hasSpace = line.text.length > at + level;
    changes.push({
      from: at,
      to: at + level + (hasSpace ? 1 : 0),
      insert: next ? "#".repeat(next) + " " : "",
    });
  }
  return lineChangeSpec(state, changes, "tandem.format.heading");
}

// A batch of line edits + the selection mapped through them (so the caret
// survives prefix insertions/removals above or around it).
function lineChangeSpec(state, changes, userEvent) {
  const cs = state.changes(changes);
  const { anchor, head } = state.selection.main;
  return {
    changes: cs,
    selection: { anchor: cs.mapPos(anchor, -1), head: cs.mapPos(head, 1) },
    userEvent,
  };
}

/**
 * Wrap the lines the selection touches in a fenced code block (a ``` fence
 * on its own line above and below), or remove an existing fence when the
 * selection starts and ends on bare fence lines with content between them.
 * A fence needs its own lines — a line prefix cannot express one — so this
 * is its own spec rather than a `toggleLinePrefixSpec` case. The inner text
 * stays selected, so the change is visible and reversible.
 */
export function fencedBlockSpec(state) {
  const { from, to } = state.selection.main;
  const first = state.doc.lineAt(from);
  const last = state.doc.lineAt(to);
  const isFence = (line) => line != null && line.text.trim() === "```";

  // An existing block is recognised either when the selection *is* the whole
  // block (its own lines carry the fences) or when the selection is the inner
  // content a previous insertion left selected (the fences sit just outside).
  let open = null;
  let close = null;
  if (isFence(first) && last.number - first.number >= 2 && isFence(last)) {
    open = first;
    close = last;
  } else if (
    first.number > 1 &&
    last.number < state.doc.lines &&
    isFence(state.doc.line(first.number - 1)) &&
    isFence(state.doc.line(last.number + 1))
  ) {
    open = state.doc.line(first.number - 1);
    close = state.doc.line(last.number + 1);
  }

  if (open && close) {
    // Already fenced: drop both fence lines (each with its own newline) and
    // keep the content between them selected.
    const inner = state.doc.sliceString(open.to + 1, close.from - 1);
    return {
      changes: state.changes([
        { from: open.from, to: open.to + 1, insert: "" },
        { from: close.from - 1, to: close.to, insert: "" },
      ]),
      selection: { anchor: open.from, head: open.from + inner.length },
      userEvent: "tandem.format.fence",
    };
  }
  // Add the fence: replace the touched lines with ``` / content / ```. With
  // an empty selection this yields an empty fenced block, caret inside.
  const blockFrom = first.from;
  const inner = state.doc.sliceString(blockFrom, last.to);
  return {
    changes: { from: blockFrom, to: last.to, insert: "```\n" + inner + "\n```" },
    selection: { anchor: blockFrom + 4, head: blockFrom + 4 + inner.length },
    userEvent: "tandem.format.fence",
  };
}

/** The TransactionSpec for a toolbar action kind (see toolbar.js). */
export function specFor(kind, state) {
  switch (kind) {
    case "bold":
      return wrapSelectionSpec(state, "**");
    case "italic":
      return wrapSelectionSpec(state, "*");
    case "strikethrough":
      return wrapSelectionSpec(state, "~~");
    case "inline-code":
      return wrapSelectionSpec(state, "`");
    case "link":
      return wrapSelectionSpec(state, "[", "](url)");
    case "heading":
      return cycleHeadingSpec(state);
    case "bullet-list":
      return toggleLinePrefixSpec(state, "- ");
    case "numbered-list":
      return toggleLinePrefixSpec(state, "1. ");
    case "task-list":
      // Checkboxes are already interactive in the preview; this action only
      // inserts the markdown syntax for one.
      return toggleLinePrefixSpec(state, "- [ ] ");
    case "quote":
      return toggleLinePrefixSpec(state, "> ");
    case "code-block":
      return fencedBlockSpec(state);
    default:
      throw new RangeError("unknown format kind: " + kind);
  }
}

// -- Note font size (toolbar control; bridge-persisted) -------------------------------
//
// The CSS custom property `--tandem-editor-font-size` on the editor root
// scales the whole pane (theme.js' "&" rule reads it). The value is a
// number of px, stepped by 1 and clamped; the 13.5 default snaps to an
// integer on the first step. Style.css carries the 13.5px default.

export const DEFAULT_EDITOR_FONT_SIZE = 13.5;
export const MIN_EDITOR_FONT_SIZE = 11;
export const MAX_EDITOR_FONT_SIZE = 20;

/** Clamp a candidate size into the sane range (13.5 for non-numbers). */
export function clampFontSize(px) {
  const n = Number(px);
  if (!Number.isFinite(n)) return DEFAULT_EDITOR_FONT_SIZE;
  return Math.max(MIN_EDITOR_FONT_SIZE, Math.min(MAX_EDITOR_FONT_SIZE, n));
}

/** The next size for a +1 / −1 step (the 13.5 default snaps onto integers). */
export function stepFontSize(px, direction) {
  const n = Number.isFinite(Number(px)) ? Number(px) : DEFAULT_EDITOR_FONT_SIZE;
  const stepped = direction > 0 ? Math.floor(n) + 1 : Math.ceil(n) - 1;
  return clampFontSize(stepped);
}


// -- DOM glue (browser only) -----------------------------------------------------------
//
// The page-global wiring below runs only where a real document + window
// exist (the bundled page loaded via loadFileURL; Node has no DOM). The
// extension set and the dirty decision it carries ARE headless-testable
// (see test/main-page-globals.test.js), because they live in the state.

/**
 * The view-level extensions, as one array the state carries:
 *  - the click handlers (Navigation interception + interactive widgets):
 *    link clicks (wikilink widgets, .tandem-link marks) arrive as
 *    link-opened and checkbox clicks dispatch the toggle transaction;
 *    the webview never leaves the bundled page;
 *  - the update listener (dirty): the full raw text on every document
 *    change (incl. checkbox toggles); setDocument is excluded by its
 *    annotation.
 *
 * These MUST be in the state, not the EditorView config's `extensions` —
 * with a supplied `state` the EditorView ignores that config array and
 * reads the `updateListener` facet and the plugin facet from
 * `this.state` (a config-only listener silently never fires; found in
 * the first human GUI pass: nothing ever autosaved).
 *
 * Widget clicks are decided here (not on the widgets) for the same
 * "real API" reason: the pinned @codemirror/view's WidgetType has no
 * eventHandlers() — the widgets carry their payload as data attributes
 * (widgets.js' TRAP note) and these handlers act on them. Returning true
 * makes the view call preventDefault and skip its own handlers (caret
 * placement and co.); no manual preventDefault needed. The checkbox
 * mousedown keeps the editor's focus (no blur/re-focus dance) — the
 * view's own mousedown usually already does; this covers the rest.
 */
export function editorViewExtensions() {
  return [
    EditorView.domEventHandlers({
      mousedown(event) {
        return checkboxToggleFrom(event.target) !== null;
      },
      click(event, view) {
        const markerFrom = checkboxToggleFrom(event.target);
        if (markerFrom !== null) {
          toggleTask(view, markerFrom);
          return true; // handled
        }
        const href = linkOpenTargetFrom(event.target);
        if (href !== null) {
          postLinkOpened(href);
          return true; // handled
        }
        return false;
      },
    }),
    EditorView.updateListener.of((vu) => {
      const text = dirtyTextForUpdate(vu.transactions, vu.state);
      if (text !== null) postDirty(text);
    }),
  ];
}

export function mountEditor(parent, { doc, vaultNotes, notePath } = {}) {
  // The view-level extensions go through the state (see
  // editorViewExtensions' doc comment) — the EditorView constructor
  // would silently drop them in the config array.
  const view = new EditorView({
    state: createEditorState({
      doc,
      vaultNotes,
      notePath,
      viewExtensions: editorViewExtensions(),
    }),
    parent,
  });

  // The formatting toolbar: page chrome over the note, in the Ligne
  // register. Buttons dispatch the pure specs above; the font-size control
  // sets the CSS custom property on the editor root and posts the value
  // (persisted by the Swift host). The toolbar's own chrome is a fixed
  // small size — it never scales with the note font.
  let fontSize = DEFAULT_EDITOR_FONT_SIZE;
  function applyFontSize(px) {
    fontSize = clampFontSize(px);
    parent.style.setProperty("--tandem-editor-font-size", fontSize + "px");
    toolbar.setFontSize(fontSize);
    return fontSize;
  }
  const toolbar = mountToolbar(
    parent,
    {
      onFormat: (kind) => view.dispatch(specFor(kind, view.state)),
      onFontSize: (action) => {
        const next =
          action === "reset"
            ? DEFAULT_EDITOR_FONT_SIZE
            : stepFontSize(fontSize, action === "increase" ? 1 : -1);
        postFontSize(applyFontSize(next));
      },
    },
    DEFAULT_EDITOR_FONT_SIZE
  );

  // Swift → JS page globals (bridge contract).
  if (typeof window !== "undefined") {
    window.setDocument = (markdownText, notePath2) =>
      view.dispatch(setDocumentSpec(view.state, markdownText, notePath2));
    window.insertText = (text) => view.dispatch(insertTextSpec(view.state, text));
    window.revealLine = (line) => view.dispatch(revealLineSpec(view.state, line));
    window.setVaultIndex = (notes) => view.dispatch(setVaultIndexSpec(notes));
    // The persisted font size arrives on `loaded`; Swift → JS only sets the
    // property (the page does not echo it back across the bridge).
    window.setFontSize = (px) => applyFontSize(px);
    // The caret: Swift pulls focus into the note when one opens, so
    // "New note → type" works without a click into the pane first (the
    // content element holds the DOM focus that keystrokes need).
    window.focusEditor = () => view.focus();
  }

  postLoaded(); // the page is ready; Swift then sends setVaultIndex/setDocument
  return view;
}

if (typeof document !== "undefined" && typeof window !== "undefined") {
  // Single self-contained page: mount into the container the bundled
  // index.html provides (falling back to <body>). The document and vault
  // index arrive from Swift over the bridge — nothing here reads files or
  // the network.
  const parent = document.querySelector("#tandem-editor") || document.body;
  mountEditor(parent);
}
