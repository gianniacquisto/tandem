// The CM6 entry for the Tandem bikeshed editor (ticket 07).
//
// Assembles the inline-live-preview editor from the existing parts:
//   - blocks.js  — the plan builder (pure: planPreview)
//   - widgets.js — the CM6 widgets (wikilink, checkbox, math, image, …)
//   - theme.js   — the Ligne day/night theme
//   - bridge.js  — JS → Swift messages
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
} from "./widgets.js";
import { ligneTheme } from "./theme.js";
import { postDirty, postLinkOpened, postLoaded } from "./bridge.js";

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

const CURATED_CODE_LANGUAGES = new Map([
  ["css", css().language],
  ["html", html().language],
  ["xhtml", html().language],
  ["svg", html().language],
  ["javascript", jsSupport.language],
  ["js", jsSupport.language],
  ["jsx", jsSupport.language],
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

  // RangeSet requires (from, to) order; equal (from, to) is fine (blocks
  // and line decorations may share a position — their `side` orders them).
  ranges.sort((a, b) => a.from - b.from || a.to - b.to);
  return Decoration.set(ranges);
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
    ...ligneTheme,
    basicSetup,
  ];
}

/**
 * A plain EditorState with the full editor configuration — no DOM.
 * The seam the headless tests drive; the browser glue builds the same
 * state and then wraps it in an EditorView.
 */
export function createEditorState({ doc = "", vaultNotes = [], notePath = "" } = {}) {
  return EditorState.create({
    doc,
    extensions: [
      ...editorExtensions(),
      ...(vaultNotes.length ? [vaultNotesFacet.of(vaultNotes)] : []),
      ...(notePath ? [notePathFacet.of(notePath)] : []),
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

// The webview never navigates: every link in the preview is a span
// (WikilinkWidget handles its own click; .tandem-link marks for markdown
// links, autolinks, and bare URLs), never an <a>. This is the pure
// decision the DOM glue applies to a click's target: the href to post as
// link-opened, or null when the click is not on a link.
export function linkOpenTargetFrom(target) {
  const mark =
    target && typeof target.closest === "function"
      ? target.closest(".tandem-link")
      : null;
  const href =
    mark && typeof mark.getAttribute === "function"
      ? mark.getAttribute("data-href")
      : null;
  return typeof href === "string" && href !== "" ? href : null;
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

// -- DOM glue (browser only) -----------------------------------------------------------
//
// Everything below runs only where a real document + window exist (the
// bundled page loaded via loadFileURL; Node has no DOM, and the tests
// never reach this layer).

export function mountEditor(parent, { doc, vaultNotes, notePath } = {}) {
  const view = new EditorView({
    state: createEditorState({ doc, vaultNotes, notePath }),
    parent,
    extensions: [
      // Navigation interception: link clicks arrive as link-opened; the
      // webview never leaves the bundled page.
      EditorView.domEventHandlers({
        click(event) {
          const href = linkOpenTargetFrom(event.target);
          if (href !== null) {
            postLinkOpened(href);
            return true; // handled
          }
          return false;
        },
      }),
      // dirty: the full raw text on every document change (incl. checkbox
      // toggles); setDocument is excluded by its annotation.
      EditorView.updateListener.of((vu) => {
        const text = dirtyTextForUpdate(vu.transactions, vu.state);
        if (text !== null) postDirty(text);
      }),
    ],
  });

  // Swift → JS page globals (bridge contract).
  if (typeof window !== "undefined") {
    window.setDocument = (markdownText, notePath2) =>
      view.dispatch(setDocumentSpec(view.state, markdownText, notePath2));
    window.insertText = (text) => view.dispatch(insertTextSpec(view.state, text));
    window.revealLine = (line) => view.dispatch(revealLineSpec(view.state, line));
    window.setVaultIndex = (notes) => view.dispatch(setVaultIndexSpec(notes));
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
