# 07 - Editor: CM6 inline live-preview bundle (JS/HTML/CSS) + build setup

Type: implementation
Status: resolved
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

Built on `bikeshed/ticket-07-editor-bundle` (merged `bikeshed/implementation` in at the end; merge commit `d7a0a26`). PR link: left for the orchestrator.

**What was built**

- `Editor/index.html` — the one HTML page: `#tandem-editor` container + `style.css` link + `editor.js` script. No network references, no fetch/XHR; the bundle self-mounts and posts `loaded`.
- `Editor/style.css` — the one CSS source file: the 10 Ligne custom properties (day + night, values from `LigneTheme.swift`, switched via `[data-theme]` — the Swift host sets `data-theme="day"|"night"` on `<html>`; day is the default), page chrome, and styles for every line class / span / widget class the planner (`blocks.js`) and widgets (`widgets.js`) emit — headings, lists (18 px/level indent), tasks, blockquotes, fenced + indented code, emphasis, inline code, links, wikilinks (broken = `--ember-deep` plain text, no underline — the binding), checkboxes, bullets, math, images + broken-image placeholder, frontmatter, tables, code-tag row, hr, the ~1 MB ceiling warning.
- `Editor/build.mjs` — esbuild build (new; `npm run build` was referenced but missing): `src/main.js` → single minified IIFE `editor.js`; folds KaTeX's CSS into `style.css` with font stacks reduced to woff2; copies the 20 KaTeX woff2 faces into `fonts/` (relative `fonts/…` URLs resolve from the CSS's output location); copies `index.html`; **generates `LICENSES.md` from the esbuild metafile** — exactly the 30 packages whose code lands in the bundle, each with its npm-declared license, the build failing on any non-MIT package (no GPL/AGPL ships).
- Committed prebuilt bundle under `Sources/Tandem/Resources/Editor/`: `index.html`, `editor.js`, `style.css`, `fonts/` (20 woff2), `LICENSES.md`.
- `Package.swift`: `resources: [.copy("Resources/Editor")]` on the `Tandem` executable target (directory structure kept intact for `loadFileURL`).

**Bundle size vs the research budget** — 1,276,575 B (≈1.22 MB) raw across 24 files: `editor.js` 979,031 B (≈318 KB gzip), `style.css` 32,655 B (hand-written 9,817 B + KaTeX CSS folded in), `index.html` 1,152 B, `LICENSES.md` 3,945 B, fonts 259,792 B (matches the research note's woff2 figure byte-for-byte). The research note's measured floor (CM6 set 611,701 B + KaTeX 557,453 B ≈ 1.14 MB) is exceeded by ~107 KB: the spec's rendered feature list mandates fenced-code highlighting for a curated set ("exact list nailed at implementation") — here the pinned `@codemirror/lang-*` packages (css, html, javascript/jsx, json, python, sql, plus markdown's own languages); the spec's other candidate names (swift, shell, yaml, toml, typescript) fall through to plain monospace per the spec's unknown-language rule. The research's minimal bundle included none of these, hence the delta. The ~1 MB budget holds in the note's own min+gzip framing (total ≈ 653 KB gzip).

**Gates (all green, re-run post-merge)**

- `npm ci && npm run build` from clean `node_modules` → reproducible: the rebuild left `git diff` clean (verified twice, including once after the merge); esbuild embeds no timestamps; `LICENSES.md` sorts every list.
- `node --check` on the output `editor.js`: passes; the bundle also executes cleanly at module level under Node (the DOM glue is guarded).
- `node --test test/`: 136/136 green (unchanged; the bundle build added no source changes).
- `swift build` clean with the new resource declaration; the built `tandem_Tandem.bundle` carries `Contents/Resources/Editor` with all 24 files and a byte-identical `editor.js`. Post-merge, SPM fetched swift-markdown as expected.

**Commits** — `bec9081` (bundle + build script + committed prebuilt bundle), `851b812` (Package.swift resource), `d7a0a26` (merge of `bikeshed/implementation`; `Package.swift` auto-merged — both sides kept: the swift-markdown package dependency AND `resources: [.copy("Resources/Editor")]`).

**Notes for ticket 08**

- Bundle location in the built product: `Bundle.module.url(forResource: "Editor", withExtension: nil)` → the directory holding `index.html`, `editor.js`, `style.css`, `fonts/`; load `index.html` via `loadFileURL` with read access to the `Editor` directory.
- The `tandem:` scheme handler must serve vault-relative paths as local files (images/attachments) and 404 missing ones — a 404 swaps in the bundle's broken-image placeholder (alt text); refuse `..` escapes.
- Set `data-theme="day"|"night"` on the page's `<html>` (system appearance) so the pane matches the window chrome; day is the default if the attribute is absent.
- GUI smoke test should verify: page loads and posts `loaded`; `setDocument`/`setVaultIndex` render the preview (headings, lists, tasks + click-toggle, quotes, fenced code with the curated highlighting, tables, math via KaTeX fonts over `file://`, broken wikilinks in ember, missing/remote images as placeholders); `dirty` carries the full text on every change incl. checkbox toggles; link clicks arrive as `link-opened` with no navigation; the ~1 MB ceiling warning renders.
