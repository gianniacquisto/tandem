// Ligne day/night theme for the CM6 editor (spec: Editor — Theme; palette
// from LigneTheme.swift, values in style.css as CSS custom properties, day +
// night via [data-theme]). The theme references the variables so the pane
// matches the window chrome; no literals live here.
import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

// Syntax colours for fenced code blocks (the curated highlight set). Kept
// inside the Ligne register: flat, one accent (the frame sage) for structure,
// ink for the rest. Unknown languages get no highlighting (plain monospace).
const syntax = HighlightStyle.define([
  { tag: [t.comment, t.quote], color: "var(--ink-dim)", fontStyle: "italic" },
  { tag: [t.keyword, t.operatorKeyword, t.controlKeyword, t.controlOperator], color: "var(--accent-deep)" },
  { tag: [t.string, t.character, t.special(t.string)], color: "var(--moss-deep)" },
  { tag: [t.number, t.bool, t.null], color: "var(--ember-deep)" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--ink)" },
  { tag: [t.definition(t.variableName)], color: "var(--ink)" },
  { tag: [t.typeName, t.className], color: "var(--accent-deep)" },
  { tag: [t.propertyName], color: "var(--ink)" },
  { tag: [t.tagName], color: "var(--accent-deep)" },
  { tag: [t.angleBracket, t.bracket, t.paren], color: "var(--ink-dim)" },
  { tag: [t.separator, t.punctuation], color: "var(--ink-dim)" },
  { tag: [t.variableName, t.propertyName], color: "var(--ink)" },
  { tag: [t.invalid], color: "var(--ember-deep)" },
]);

export const ligneTheme = [
  syntaxHighlighting(syntax),
  EditorView.theme(
    {
      "&": {
        backgroundColor: "var(--paper)",
        color: "var(--ink)",
        // The note font size — set on the editor root by the toolbar's
        // font-size control (default 13.5px in style.css) and persisted
        // across launches via the bridge. Not a literal here so the whole
        // pane scales as one.
        fontSize: "var(--tandem-editor-font-size)",
        height: "100%",
      },
      ".cm-scroller": {
        fontFamily: '-apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif',
        lineHeight: "1.35",
        padding: "24px 0",
      },
      // 720 pt prose column, centred (prototype: h padding 28, v padding 24).
      ".cm-content": {
        maxWidth: "720px",
        margin: "0 auto",
        padding: "0 28px",
        caretColor: "var(--ink)",
        // An unbreakable run (a long URL, an endless word) wraps instead of
        // forcing a horizontal scrollbar.
        overflowWrap: "anywhere",
      },
      ".cm-line": {
        padding: "0 2px",
        minHeight: "1.35em", // stable line height: classes, never token visibility
      },
      ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--ink)" },
      "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection":
        { backgroundColor: "color-mix(in srgb, var(--accent) 28%, transparent)" },
      ".cm-activeLine": { backgroundColor: "transparent" },
      ".cm-gutters": { display: "none" },
      // Fenced code blocks (blocks.js line classes).
      ".tandem-code-tag": {
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        fontSize: "10.5px",
        color: "var(--ink-dim)",
        textAlign: "right",
        backgroundColor: "var(--parchment)",
        borderRadius: "9px 9px 0 0",
        border: "1px solid var(--sand)",
        borderBottom: "none",
        padding: "6px 10px 2px",
      },
      ".tandem-code-line": {
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        fontSize: "12px",
        backgroundColor: "var(--parchment)",
        borderLeft: "1px solid var(--sand)",
        borderRight: "1px solid var(--sand)",
        padding: "0 10px",
      },
      ".tandem-code-end": {
        backgroundColor: "var(--parchment)",
        border: "1px solid var(--sand)",
        borderTop: "none",
        borderRadius: "0 0 9px 9px",
        padding: "6px 10px 10px",
      },
      // Indented code blocks.
      ".tandem-codeblock": {
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        fontSize: "12px",
        backgroundColor: "var(--parchment)",
        border: "1px solid var(--sand)",
        padding: "0 10px",
      },
      ".tandem-codeblock-first": { borderRadius: "9px 9px 0 0", padding: "6px 10px 0" },
      ".tandem-codeblock-last": { borderRadius: "0 0 9px 9px", padding: "0 10px 10px" },
      // Blockquote (parchment panel, 9 corners, sand hairline).
      ".tandem-quote": {
        backgroundColor: "var(--parchment)",
        border: "1px solid var(--sand)",
        padding: "0 14px",
      },
      ".tandem-quote-first": { borderRadius: "9px 9px 0 0", padding: "10px 14px 0" },
      ".tandem-quote-last": { borderRadius: "0 0 9px 9px", padding: "0 14px 10px" },
      // Headings (H1 20 bold, H2 16 semibold, H3+ 14 semibold). No extra
      // padding: line height comes from the class's font size alone, so a
      // marker appearing/disappearing shifts height by the class, never by
      // hidden tokens (spec invariant 2).
      ".tandem-h1": { fontSize: "20px", fontWeight: "700" },
      ".tandem-h2": { fontSize: "16px", fontWeight: "600" },
      ".tandem-h3, .tandem-h4, .tandem-h5, .tandem-h6": {
        fontSize: "14px",
        fontWeight: "600",
      },
      // Inline code (12 pt monospaced, sand @ .45 background).
      ".tandem-code": {
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        fontSize: "12px",
        backgroundColor: "color-mix(in srgb, var(--sand) 45%, transparent)",
        borderRadius: "3px",
        padding: "0 3px",
      },
      // Links (Ligne restraint: colour carries the affordance — no underline).
      ".tandem-link": { color: "var(--accent)", cursor: "pointer" },
      ".tandem-wikilink": { color: "var(--accent)", cursor: "pointer" },
      ".tandem-wikilink-broken": { color: "var(--ember-deep)", cursor: "pointer" },
      // Math.
      ".tandem-math": { color: "var(--accent-deep)" },
      ".tandem-math-error": {
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        color: "var(--ink)",
      },
      ".tandem-display-math": {
        padding: "6px 0",
        overflowX: "auto",
        color: "var(--accent-deep)",
      },
      // Task items.
      ".tandem-task-done": { color: "var(--ink-dim)" },
      // Frontmatter placeholder.
      ".tandem-frontmatter": {
        color: "var(--ink-dim)",
        fontSize: "11px",
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        padding: "2px 0 6px",
      },
      // Broken-image placeholder (dashed [3,3] ember border, 9 corners,
      // max width 300, padding 10 — per the prototype).
      ".tandem-img": { maxWidth: "100%", display: "block", margin: "6px 0" },
      ".tandem-img-broken": {
        border: "1px dashed var(--ember)",
        borderRadius: "9px",
        maxWidth: "300px",
        padding: "10px",
        margin: "6px 0",
        color: "var(--ember-deep)",
        fontSize: "11.5px",
        fontWeight: "500",
      },
      ".tandem-img-broken .tandem-img-ref": {
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        fontSize: "10.5px",
        color: "var(--ink-dim)",
        fontWeight: "400",
      },
      // Horizontal rule (1 pt sand hairline, Ligne restraint).
      ".tandem-hr": {
        border: "none",
        borderTop: "1px solid var(--sand)",
        margin: "10px 0",
        height: "0",
      },
      // Soft per-note ceiling warning (~1 MB): a quiet ember row at the top.
      ".tandem-ceiling-warning": {
        color: "var(--ember-deep)",
        fontSize: "11px",
        fontWeight: "500",
        borderBottom: "1px solid var(--ember)",
        padding: "2px 0 6px",
        margin: "0 0 8px",
      },
      // Table widget (prototype: cells 12.5, header semibold, 1pt ink under
      // header, 1pt sand between rows).
      ".tandem-table": {
        borderCollapse: "collapse",
        maxWidth: "100%",
        margin: "6px 0",
        fontSize: "12.5px",
      },
      ".tandem-table th": {
        fontWeight: "600",
        textAlign: "left",
        padding: "6px 10px",
        borderBottom: "1px solid var(--ink)",
      },
      ".tandem-table td": {
        padding: "6px 10px",
        borderBottom: "1px solid var(--sand)",
        verticalAlign: "top",
      },
    },
    { dark: false }
  ),
];
