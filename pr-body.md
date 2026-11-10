UI-22 retires the second editor: the iframe, `editor-frame.html`, the `window.webkit` shim, the whole `Editor/` subproject, the 6.2 MiB committed bundle at `app/renderer/public/editor/`, and the two macOS-only CI gates that rebuilt it. The renderer is one page, one parser, one build.

## What changed

- **Deleted the old path.** `BikeshedEditor.tsx`, `editor-frame.html`, `src/editor/frame.ts`, `Editor/` (build, sources and 21 tests), `app/renderer/public/editor/`, `shell/editor.ts`'s protocol half, and their tests. `electron.vite.config.mts` builds `index.html` alone; `app.yml` drops both `Editor/package-lock.json` cache paths and the bundle gates.
- **Moved the preference.** The font-size helpers live in `shell/fontSize.ts` (default 16, clamp 11-20, load/save), with `tests/fontSize.test.ts` beside them. No control is added; UI-27 owns it.
- **KaTeX.** `katex@0.19.0` joins `dependencies`. `shell/math.ts` renders TeX with `throwOnError: false`; inline runs and `$$` blocks draw KaTeX when the caret is elsewhere and their source while the caret is in the block, so the row stays editable. `katex/dist/katex.min.css` is imported from the renderer, and the renderer's `assetsInlineLimit` is 0 so the 20 woff2 faces are files, not `data:` URLs.
- **Mermaid.** `mermaid@12.2.0` joins `dependencies`. `shell/mermaid.ts` carries the bundle's render path (async render, theme+source cache, injected render seam, token palette with day fallbacks), `MermaidDiagram.tsx` draws a ```mermaid fence, and a system scheme change re-renders mounted diagrams. Reveal-to-edit shows the fence's source while focused; a failed parse leaves the source under `.tandem-mermaid-error`.
- **Policy tightens.** `frame-src 'self'` becomes `frame-src 'none'`; `frame-ancestors 'self'`, `font-src 'self'` and the rest stay.
- **Licence record.** `scripts/third-party-notice.mjs` generates `app/THIRD-PARTY.md` from the production lockfile closure (123 packages), with the bundle's allowed-licence set and SPDX handling and each package's verbatim text. It is staged into `build/`, carried by `extraResources`, and asserted by `check-artifact.mjs`.
- **Gates.** `scripts/check-renderer.mjs` proves one page, no editor frame/chunk/bundle copy, exactly 20 KaTeX woff2 faces and no inlined font. `app.yml` runs it after the build and regenerates the notice and fails on a diff.
- **Docs and prose.** `app/README.md`, `app/CONVENTIONS.md`, `tests/shell.test.ts`, `docs/screenshots/README.md` and the scene description follow the code. The retired-path gate gained `Editor/` and `editor-frame.html`; it also caught live prose in `CONTRIBUTING.md`, the root `README.md`, the PR template and `agents/builder.md`, which now name no retired path. The provenance comments that cite the retired editor sources remain, under allowlist entries, as the git-history record.
- **Tests.** `app/tests/mermaid.test.ts` (SVG mounts, rejection leaves the source, cache reuse vs theme change, token mapping), `app/tests/math.test.ts` (inline, display, invalid TeX), `app/tests/fontSize.test.ts`, `app/tests/security.test.ts`, and a new `bikeshed.shots.ts` case for a ```mermaid fence and a `$$` block. No `frameLocator` remains in the shot suite.

## Measured sizes

- Committed bundle removed: **6,458,839 B** across 24 files (`app/renderer/public/editor`).
- `out/renderer` at base `1b00d06`: **7,563,593 B** / 29 files. After: **11,602,661 B** / 148 files. The growth is mermaid's own per-diagram lazy chunks plus KaTeX; only the CodeMirror bytes leave. UI-24 owns the split and the measurement that proves it. The brief's "near 7.0 MB" estimate does not hold on this head.
- `dist/mac-arm64/Tandem.app` (`electron-builder --dir`, macOS arm64): **272 MB** before, **277 MB** after.
- `app/THIRD-PARTY.md`: 227,098 B, 123 production packages.

## Verified locally

```
cd app && npm run typecheck
cd app && npx vitest run tests/fontSize.test.ts tests/math.test.ts tests/mermaid.test.ts tests/security.test.ts tests/shell.test.ts tests/inline-view.test.ts tests/bikeshed.chrome.test.ts tests/shots.catalogue.test.ts
cd app && npm run build && node scripts/check-renderer.mjs
cd app && node scripts/third-party-notice.mjs --check
cd app && TANDEM_SHOTS_HOME=../Fixtures/port/Home npx playwright test tests/shots/bikeshed.shots.ts
bash Scripts/check-retired-paths.sh
```

The full `npm test` also ran on this head; its one failure was the colour-literal guard rejecting the mermaid day fallbacks, which now has a documented exception in `tests/shell.test.ts` and passes in the filtered run above.

## Merge Danger

Door: **two-way**. The change is merge-revertible and the retired tree stays in git history; no data migration and no external state. A revert restores the bundle and its gates.

Blast Radius: **high**. It deletes a whole subproject plus the committed bundle, changes the renderer's production dependency graph, tightens the CSP, changes the packaged artifact's contents, and rewrites two CI gates. A mistake can break rendering, packaging, or an installed app's font loading, which is why the reviewer's low-blast auto-merge rule should not cover it.
