# 07 - Editor: CM6 inline live-preview bundle (JS/HTML/CSS) + build setup

Type: implementation
Status: claimed
Blocked by: none (independent of the core; integrates with the GUI, ticket 08, only through the bridge contract)

## What to build

The editor surface: a small static bundle — one HTML page, one prebuilt JS bundle, one CSS file — CodeMirror 6 with inline live preview, built with esbuild, shipped under the GUI target's resources. The Swift side (ticket 08) hosts it in a WKWebView. The binding contract between this ticket and ticket 08 is the bridge contract note — do not deviate from it without reporting the deviation.

Scope = the spec's sections: "Editor — per ticket 01's research verdict" (stack, live-preview mechanism, wikilinks in the editor, bridge, images & attachments, math, theme, scale, packaging) and "Rendered feature list (normative, per ticket 05)" — both the rendered and the not-rendered lists are normative.

Context (pointers):
- Spec: `.scratch/bikeshed/spec.md` (the only normative input)
- Bridge contract (binding for this ticket AND ticket 08): `/private/var/folders/4x/kz_77l9s0lddszts3p0k6b940000gn/T/opencode/bikeshed/notes/bridge-contract.md`
- Research verdict, pinned versions, bundle sizes: `/private/var/folders/4x/kz_77l9s0lddszts3p0k6b940000gn/T/opencode/bikeshed/notes/editor-research.md` (from branch `research/bikeshed-editor-tech`)
- Fact files: `.scratch/bikeshed/research/obsidian-wikilink-facts.md`, `.scratch/bikeshed/research/rendered-markdown-facts.md`
- Visual reference (broken-link ember tone, palette, designed states): `/private/var/folders/4x/kz_77l9s0lddszts3p0k6b940000gn/T/opencode/bikeshed/notes/ui-prototype.md` (from branch `prototype/bikeshed-ui`); Ligne palette values in `Sources/Tandem/LigneTheme.swift`

## Acceptance

- [ ] Checked-in pinned `package.json` + lockfile (npm metadata is the source of truth for versions): codemirror 6.0.2, @codemirror/state 6.7.6, @codemirror/view 6.43.14, @codemirror/language 6.13.1, @codemirror/lang-markdown 6.5.2, @lezer/markdown 1.8.0, plus KaTeX — per the spec's Packaging
- [ ] esbuild build script; prebuilt bundle (one HTML, one JS, one CSS) committed under the GUI target's resources; `Package.swift` resource declaration added; license texts (CM6/Lezer MIT, KaTeX MIT) kept with the bundle; no GPL/AGPL anywhere
- [ ] Inline live preview: incremental `@lezer/markdown` parse + `Decoration.replace({})` over marker ranges; the two normative invariants hold — raw markdown is the source of truth (decorations view-only); line height depends on CSS classes, never on token visibility; block decorations (tables, display math, images) come from a StateField, not a ViewPlugin
- [ ] Wikilink extension per the grammar: `[[target[#fragment][|alias]]]` — first-`#`-then-first-`|` split, first `]]` closes; linkified per the resolution rules against the file set passed over the bridge (case-insensitive, exactly-one rule); broken links visually distinct (ember tone) but unobtrusive; not links: unclosed `[[`, empty target, same-note `[[#Heading]]`, embeds `![[…]]`, anything inside code fences or inline code
- [ ] Rendered per the normative list: GFM floor (frontmatter stripped from the preview, never parsed); interactive task checkboxes (click toggles `- [ ]` ↔ `- [x]` in the document — one-character transaction, buffer stays source of truth); fenced-code highlighting for the curated set (unknown language → plain monospace); KaTeX inline + display math with Obsidian-style trigger rules (a currency `$` doesn't start math; parse errors leave the raw math visible); local images via the `tandem:` scheme (missing file → broken-image placeholder with alt text; container-width cap); markdown links resolved to vault-file existence (path, case-insensitive)
- [ ] Not rendered, literally per the normative list: mermaid blocks (highlighted code), callouts, footnotes, emoji shortcodes, raw HTML (styled, never executed), embeds, tags, highlights, comments; no code-block copy button
- [ ] Bridge JS side exactly per the bridge contract note: `loaded` / `dirty` (full text) / `link-opened` / `checkbox-toggled` out; `setDocument` / `insertText` / `revealLine` / `setVaultIndex` in; navigation intercepted (link clicks reach Swift, the webview never leaves the bundled page); JS has no file or network access — remote `http(s)` images rewritten to the broken-image placeholder before render, never fetched
- [ ] Soft per-note ceiling: visible warning around ~1 MB
- [ ] Ligne day/night theme so the pane matches the window chrome (palette from `LigneTheme.swift`)
- [ ] No web workers; `loadFileURL` from the app bundle works
- [ ] Build is reproducible: `npm ci && npm run build` regenerates the committed bundle byte-identically (or with documented drift); `node --check` passes on the output bundle
- [ ] `swift build` clean with the new resource declaration

## Resolution

(to be filled at merge)
