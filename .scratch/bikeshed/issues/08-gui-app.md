# 08 - GUI: Bikeshed tab, tree sidebar, editor host, watcher, backlinks dock

Type: implementation
Status: resolved
Blocked by: 06 (needs the `Bikeshed` core API), 07 (hosts the committed editor bundle)

## What to build

The app face for the bikeshed: the Main window's second tab (Todos | Bikeshed), the tree sidebar with search, the WKWebView editor host with the Swift side of the bridge, the recursive FSEvents watcher, the backlinks dock, and the app-side vault protocol behaviour (autosave, conflict banner, Trash deletion, new note / rename / move). The menu bar item stays todos-only.

Scope = the spec's sections: "UI shell — per ticket 02's verdict and the prototype", "Watcher", the app-side of "Vault protocol" (autosave, external changes/conflict, deletion), "Search and backlinks" (UI side), the window-sizing decision, and "Editor — … Bridge" (the Swift side). The spec is the only normative input; where the real core API (ticket 06) or bundle (ticket 07) differs from the spec in signature detail, the in-tree code wins for signatures and the spec wins for behaviour.

Context (pointers):
- Spec: `.scratch/bikeshed/spec.md`
- Bridge contract (binding for this ticket AND ticket 07): `/Users/gianniacquisto/projects/tandem-worktrees/bikeshed/notes/bridge-contract.md`
- In-tree after 06/07 merge: `Sources/TandemCore/Bikeshed*.swift` (the real core API), the committed editor bundle under the GUI target's resources
- Conventions to mirror: `Sources/Tandem/` (Model, MainView, MenuBar, PopoverFace, StoreWatcher, LigneTheme, LigneControls, SingleInstance)
- Exploration notes: `/Users/gianniacquisto/projects/tandem-worktrees/bikeshed/notes/gui-structure.md`, `notes/ui-prototype.md` (the visual reference; branch `prototype/bikeshed-ui` available for deeper reference)

## Acceptance

- [ ] Main window gains the second tab — Todos | Bikeshed — in the window header; menu bar item untouched; one window, one size for both tabs (no resize on tab switch; default 1060×680; user-resizable; minimum width fits the Bikeshed face); the Todos face keeps its 540 pt centred column
- [ ] Observable model for the Bikeshed tab mirroring `TandemModel`'s shape; the `Bikeshed` core type is the only vault logic the GUI holds (no note logic in views)
- [ ] Tree sidebar: search field on top, nested tree below — folders first, alphabetical, `.md` notes only (no attachments, no hidden files, no lockfile); "new note" action creates at the vault root; note create/rename/move with name validation (reject `/`, `#`, `|`; rename onto a taken name refused, nothing written — core error surfaced)
- [ ] Search while typing: the tree is replaced by full-vault results — case-insensitive substring over title + body, one row per note, vault-path order; title hits lead with the note's first line, body hits show a snippet around the first match; clicking a result opens the note and clears the query
- [ ] Editor column: WKWebView loads the bundled editor page (`loadFileURL`); Swift side of the bridge exactly per the bridge contract (`WKScriptMessageHandler` in, `callAsyncJavaScript` out); navigation intercepted
- [ ] `tandem:` WKURLSchemeHandler confined to the vault root (refuses `..` escapes); serves local images/attachments; missing file → the bundle's broken-image placeholder; images capped at container width; remote `http(s)` never fetched
- [ ] Autosave: JS `dirty` → Swift debounces ~1 s, performs the core write, records it for self-write dedup; also on note switch, window blur, ⌘S; at most a transient "saving…" — no persistent unsaved badge; a failed write keeps the buffer, surfaces the error, and retries
- [ ] Conflict policy: the buffer tracks the on-disk hash from `read`; clean buffer → external change adopted silently (including self-writes deduped by the recorded hash); dirty buffer → banner [Overwrite with mine] / [Take theirs], pending autosave withheld until chosen
- [ ] Broken-link click (via `link-opened`): creates an empty note at the link's path (bare → vault root; qualified → that path, folders created as needed) and opens it, no confirm dialog; resolved wikilink and internal markdown link → open the target note in the editor; attachment link → OS default app; external link → OS default handler
- [ ] Backlinks dock: collapsible, at the bottom of the editor column, count in the header; each row = the linking note + the line containing the wikilink that resolves to the open note; empty reads "Nothing links here yet."; broken links contribute nothing
- [ ] Deletion (note or folder): moves to the macOS Trash (recoverable) via the GUI, falling back to the core's permanent delete; no confirm beyond the standard
- [ ] Designed states: empty vault (no welcome note, designed placeholder — lazy folder creation, never an error state) and no-note-selected (designed placeholder)
- [ ] Watcher: one recursive `FSEventStream` on the vault root per the spec's behaviour contract — flags `FileEvents | NoDefer | UseCFTypes | WatchRoot`, `sinceWhen = kFSEventStreamEventIdSinceNow`, latency ≈ 0.2 s, background dispatch queue; each batch coalesced into one "vault changed" pass on the main actor (rebuild the tree; reload the open note only if its path is in the batch); self-write dedup; `RootChanged` / stream start / app launch → full rescan; `StoreWatcher` untouched
- [ ] Editor palette follows the Ligne theme in day and night
- [ ] Packaging: the editor resources (bundle + KaTeX assets) and the swift-markdown NOTICE are added to `Scripts/make-app-bundle.sh`'s copy list and to the CI bundle-structure verification — otherwise they silently ship missing in tag runs
- [ ] `swift build -c release` clean; `swift test` green; the app launches, the tab switches without resizing, and the Bikeshed face works end to end against a real (temp-seeded) vault — record the smoke results in Resolution for the human's manual pass per the spec

Note: the platform floor stays `.macOS(.v27)` — the spec prose says "macOS 26+" in places, but the package floor (and the CI runner) are the truth. Do not lower it.

## Resolution

Built on `bikeshed/ticket-08-gui-app` (base `bikeshed/implementation` @ `407adf4`, i.e. 06 + 07 merged). Commits: `7718dbf` (model + watcher + editor host), `7f6bc4d` (face + tab integration + packaging), `4a36ad0` + `a7cc82f` (verify-and-finish fixes — see below). Delivered via PR #7 — https://github.com/gianniacquisto/tandem/pull/7.

**What was built**

- `BikeshedModel.swift` (new) — the observable model for the Bikeshed tab, mirroring `TandemModel`'s shape: deliberate `@Observable` singleton, everything on the main thread by convention, `private(set)` state, thin non-throwing op wrappers that surface the core's error text verbatim. Holds the `Bikeshed` core value (the ONLY vault logic the GUI holds), the tree/selection/expanded state, debounced off-main search + backlinks (detached tasks, stale results dropped), open/create/rename/delete, the editor buffer + `baselineHash` from `read`, ~1 s autosave debounce + 2 s failed-write retry (buffer kept), content-hash conflict with mine/theirs, link-opened routing (resolved → open, path-existence → note in editor / attachment in OS app, external → OS handler, broken → create + open), and the watcher pass with self-write dedup by the recorded `WriteRecord.hash`.
- `VaultWatcher.swift` (new) — one recursive `FSEventStream` per the spec's behaviour contract: `FileEvents | NoDefer | UseCFTypes | WatchRoot`, `sinceWhen = kFSEventStreamEventIdSinceNow`, 0.2 s latency, background dispatch queue; each batch coalesced into one main-queue `Change { paths, rootChanged }`; `StoreWatcher` untouched.
- `EditorHost.swift` (new) — the Swift side of the bridge (notes/bridge-contract.md) and the persistent WKWebView (owned by the model, so it survives tab switches — no page reload). JS→Swift: `WKScriptMessageHandler` on `"tandem"` for `loaded` / `dirty` / `link-opened` / `checkbox-toggled`. Swift→JS: `setDocument` / `insertText` / `revealLine` / `setVaultIndex` via `callAsyncJavaScript`, gated on `loaded` (state re-sent at load). `tandem:` `WKURLSchemeHandler` confined to the vault root (`.`/`..`/empty components refused, containment re-check, 404 → the bundle's broken-image placeholder, no-store). Navigation intercepted — non-`file:` navigations cancelled, the webview never leaves the bundled page. `data-theme="day|night"` on `<html>` from the window appearance.
- `BikeFace.swift` (new) — the face: `MainTab` + `BikeTabControl` (Todos | Bikeshed, header), `BikeFaceView` (error banner, designed empty-vault / no-note-selected states, the 232 pt sidebar, the editor column: 42 pt title bar with folder path + title + transient "saving…", the conflict banner, the persistent webview pane, the collapsible backlinks dock with count + "Nothing links here yet."), `BikeTreeView` (search field on top, tree↔results swap while typing, folder rows with new-note/delete context menus, note rows with rename…/delete, rename sheet). Esc clears the query; ⌘S flushes the save. No note logic — every action forwards to the model.
- `MainView.swift` — second tab in the header; Todos face kept as its 540 pt centred column; min window 860×560 (fits sidebar + editor).
- `TandemApp.swift` — default 1060×680; `BikeshedModel.shared` in the environment.
- `LigneControls.swift` — `LigneBanner` gained optional `actions` + dismiss (the conflict banner's [Overwrite with mine] / [Take theirs]).
- `Scripts/make-app-bundle.sh` — copies `tandem_Tandem.bundle` (error if the build did not emit it) + `NOTICE.txt` into the .app.
- `.github/workflows/release.yml` — the bundle-structure verify step now checks `NOTICE.txt` + `Editor/{index.html,editor.js,style.css,fonts/,LICENSES.md}` inside the assembled .app.

**Acceptance re-check (16 items)** — all satisfied.

1–2. Tabs/window/model — the header tab control, one window, default 1060×680, min 860×560, Todos 540 pt column; `MenuBar.swift` untouched (verified by diff). The model mirrors `TandemModel` (singleton, private init, watcher attach, cold-start reload, thin ops, verbatim `TandemError` text); the views draw only.
3–4. Tree + search — core `list()` order (folders first, alphabetical, `.md` only); new note at root (sidebar +, empty-state button, folder-row menu); rename/move via the core's validated `rename` (taken name / bad name refused, nothing written, message surfaced); search debounced 0.2 s, off-main, tree replaced by results, click opens + clears, re-run on vault change while active.
5–6. Editor + `tandem:` — `loadFileURL` scoped to the `Editor` dir; the bridge per the contract (one documented payload-shape deviation below); scheme handler confined, 404 on missing, no-store; remote `http(s)` rewritten by the JS before render (bundle) and never fetched.
7–8. Autosave + conflict — `dirty` → 1.0 s debounce → core write → recorded for self-write dedup; also on note switch (`open` flushes first), window blur (`NSWindow.didResignKeyNotification`), ⌘S; at most the transient "saving…"; failed write keeps the buffer, surfaces the error, retries at 2 s. Clean buffer adopts external changes silently; self-writes deduped by hash; dirty → banner, autosave withheld (`performSave` gates on `!conflict`).
9. Link routing — scheme URLs → OS; bare `#fragment` inert (v1); wikilink resolve → open; path-existence fallthrough (note → editor, attachment → OS app); broken → empty note created at the link's path (bare → root; qualified → folders created by the core) + opened, no confirm.
10–12. Backlinks dock / Trash deletion / designed states — collapsible dock at the bottom of the editor column, count in the header, core rows (note + line), "Nothing links here yet."; `FileManager.trashItem` first, core's permanent `delete` as fallback, destructive context-menu role only; empty-vault and no-note-selected placeholders with no welcome note; the folder is created lazily (core init + watcher start), never an error state.
13. Watcher — flags/latency/queue per spec; one coalesced main-queue pass per batch; tree rebuild + open-note reload only if the path is in the batch (or `RootChanged`); self-write dedup; full rescan at stream start and app launch (cold-start `reloadTree`).
14. Day/night — `Palette.forScheme` throughout the face; `data-theme` follows the window appearance.
15. Packaging — script + CI verify per item 15 above.
16. Gates + smoke — see below.

**Documented deviation (reported, not silent)** — the bridge-contract note writes the `setVaultIndex` payload as `[{path: string}]`, but the committed bundle (ticket 07, 136/136-tested) consumes a flat `string[]` (`resolveWikilink(target, notes)` iterates raw path strings; the page-global tests exercise `["note.md"]`). Per this ticket's rule — in-tree code wins for signatures, spec/contract wins for behaviour — the Swift side sends what the bundle consumes. Recorded in `EditorHost.sendVaultIndex`'s doc comment and here; the contract note has since been reconciled to the flat `string[]` shape (orchestrator, post-merge — code was correct, the note was wrong).

**What verify-and-finish found and fixed**

1. **SIGSEGV on the first FSEvent batch** (crash report `Tandem-2026-10-16-161140.ips`). `VaultWatcher.handle` read `eventPaths` as a C array of object pointers; with `UseCFTypes` it is a **CFArray of CFStrings** (verified empirically with a standalone FSEvents probe: the raw pointer type-checks as `CFArray` and bridges to the event paths; the C-array read produced a garbage "CFString" → `objc_retain` fault). The first smoke run of the assembled app died exactly this way — alive 12 s (no events yet), dead on the first terminal write. Fixed by bridging the CFArray.
2. **`setVaultIndex` not sent on vault change** — the contract says "sent on load and after every vault change"; `vaultChanged` only rebuilt the tree, so a broken link in the open note would stay broken until the note was reopened even after its target appeared. Now sent on every pass.
3. **`callAsyncJavaScript` off-main** — the SDK derives it from `WK_SWIFT_UI_ACTOR` (i.e. `@MainActor`); `EditorHost.send` called it from a bare `Task {}` (global executor) — legal in Swift 5 mode, contract violation at runtime. Now hops `@MainActor`.
4. **Superseded backlink results could land stale** — `refreshBacklinks` cancels the previous task but `runBacklinks` never checked `Task.isCancelled` (its own comment promised the guard; `runSearch` had it). Added.

Items 1–3 in `a7cc82f`; item 4 in `4a36ad0`.

**Gates (post-fix, re-run)**

- `swift build -c release`: clean.
- `swift test`: 130/130 (12 suites; scale sentinel 94 MB / 280 notes ≈ 11 s, bound 45 s).
- `cd Editor && npm ci && node --test test/`: 136/136 (the bundle was never touched by this ticket).

**Assembled .app** — `Scripts/make-app-bundle.sh .build/release/Tandem .build/release/tandem-mcp 0.0.0 <tmp>` → `Tandem.app`. Every CI verify-step check passes against it: both binaries, `Info.plist` (0.0.0), `AppIcon.icns`, `TandemBike.jpg`, `NOTICE.txt`, `tandem_Tandem.bundle/Contents/Resources/Editor/{index.html, editor.js, style.css, fonts/ (20 woff2), LICENSES.md}`; `codesign --verify --strict` passes; in-bundle `tandem-mcp` exits cleanly on stdin EOF; the app's `editor.js` is byte-identical to the build output. `Bundle.module` resolution in the assembled app confirmed against the generated release accessor (it looks under `Bundle.main.resourceURL` → `Contents/Resources/tandem_Tandem.bundle`).

**Smoke (as much as the shell allows)**

Seeded a realistic vault at `~/.tandem/bikeshed` (did not exist before this ticket): folders; wikilinks — resolved qualified/bare, broken, duplicate-basename (genuinely ambiguous), aliased `[[a|b]]`, fragment `[[a#H]]`, case-insensitive; task lists (checked + unchecked); fenced code (curated `js` + unknown language); KaTeX inline + display; images (real local file, missing local, remote `https:`); leading frontmatter; a hidden file and a non-`.md` attachment (both tree-excluded).

- **Pre-fix**: launched the first assembled app; alive 12 s; SIGSEGV on the first external write (see findings).
- **Post-fix (`a7cc82f`)**: relaunched from the reassembled .app — launched, stayed alive **~2.5 min**, survived repeated real FSEvent batches from the terminal (create, append, delete, recreate — the agent-write path, coalesced main-queue passes, no crash, no runaway), no new crash reports, clean SIGTERM exit with the single-instance pidfile released. Stale-claim reclamation also exercised (the crashed run left a pidfile; the next launch took it over).
- Headlessly verified: process liveness, watcher-batch survival, single-instance claim/release, .app structure/signature, bundle resolution path. What the shell cannot verify (no GUI input, and this shell has no assistive-access permission for System Events window queries) is recorded below.

**Manual GUI checklist (the human's pass per the spec)** — re-seed the vault first (it was cleaned after the smoke; see "what remains" below). **All 10 items PASSED on the reassembled release .app, 2026-10-17** — four bugs were found during the pass (see below); items 1, 2, 6 passed on the first pass, the rest after the four fixes, final confirmation from the user 2026-10-17.

1. Editor loads the bundled page — **passed**: open the Bikeshed tab, the CM6 live-preview page renders (headless: the bundle resolves where `EditorHost` expects it; the JS glue is 136/136-tested).
2. Typing → autosave ~1 s → file on disk — **passed**: type in a note, wait ~1.5 s, `cat` the file; also on note switch, window blur, ⌘S.
3. Agent-written file → tree + open note update within ~0.5 s, no self-write loop — **watcher half headlessly verified** (real batches processed without crash); **passed**: open a note, write/modify a file from the terminal, watch the tree (and the open note adopt) update promptly, and confirm no dirty re-fire loop after the app's own autosave.
4. Broken-link click creates the note — **passed**: click the ember `[[broken-link]]` → an empty `broken-link.md` appears at the root and opens; qualified → the folder is created.
5. Checkbox toggle persists — **passed**: click a task checkbox, wait for autosave, verify `- [x]` on disk.
6. Math/images/code highlighting render — **passed**: open the seeded Welcome — KaTeX inline + display, local image, broken-image placeholder (missing + remote), `js` highlighted, unknown language plain monospace.
7. Conflict banner (mine/theirs) — **passed**: type in a note, edit the file on disk while the buffer is dirty, pick [Overwrite with mine] / [Take theirs]; verify the pending autosave is withheld until the choice.
8. Trash deletion recoverable — **passed**: right-click → Delete → the note is in the macOS Trash.
9. Day/night matches the window — **passed**: toggle the system appearance; the webview pane follows.
10. Tab switches without resizing — **passed**: Todos ↔ Bikeshed; the window frame stays put; the Todos face keeps its 540 pt centred column.

**What remains in `~/.tandem/bikeshed/` after cleanup** — nothing (the empty vault directory itself remains; it is the app's default location and is created lazily anyway). The todos store (`todos.json`, `tandem.lock`) was never touched; `~/.tandem/app.pid` released on clean exit.

**Post-merge (two-axis code review, base `main`)** — Standards + Spec axes run in parallel; all actionable findings fixed in one pass, final review tip `7ebd773`. The core (ticket 06) was untouched — it is the single-resolver reference.

Fixed:

- **Attachment + broken-bare-wikilink routing was wrong vs the spec** — link-opened routing now matches the spec's rules exactly (resolved → open in editor; path-existence fallthrough → note in editor / attachment in OS app; external → OS handler; broken → create + open). `722851d`.
- **Vault path hygiene was duplicated** — the containment/normalisation logic now lives in one shared helper (`VaultPaths.swift`) used by the scheme handler and the model, instead of two copies that could drift. `722851d`.
- Bundle-side packaging (LICENSES texts, dead dep/widget, `ts` flag) and the CI editor gate — see ticket 07's post-merge section (`5a602af`, `7ebd773`).
- `GLOSSARY.md` vault wording aligned with the grammar (`7ebd773`).
- **Core-parity grammar fix (`03afee4`)** — empty/whitespace alias/fragment now absent in the JS grammar to match the core; the core itself did not change. Node 144 → 149.

Deliberately kept (judgement calls, documented): trash-fallback to permanent `delete` when `FileManager.trashItem` fails; flat `setVaultIndex` payload (contract note reconciled, not code); Esc clears the search query; the window-resign observer is never removed (one per app lifetime, no leak); code highlighting for swift/shell/yaml/toml left as plain monospace (no pinned packages to justify adding them).

**Commits** — `7718dbf`, `7f6bc4d` (the prior session's work), `4a36ad0`, `a7cc82f` (verify-and-finish). Branch tip `a7cc82f`, pushed. PR link: left for the orchestrator.

**GUI-pass bugs (found by the human pass, fixed on `bikeshed/implementation`)** — all four were editor-bundle bugs, verified against the reassembled release .app before re-test; traps documented at each seam so none can silently regress:

1. **Autosave never fired** — the view-level extensions (`updateListener`, `domEventHandlers`) must live in the **state**, not the `EditorView` config's `extensions`: the pinned @codemirror/view only merges `config.extensions` when it creates the state itself, and `EditorHost` builds the state via `createEditorState` and passes it in. `edc0e41`.
2. **Editor blank after load** — WebKit's `callAsyncJavaScript` exposes the `arguments:` dictionary's keys as **bare identifiers**, not `arguments.*` properties; the `send` bodies were written in the `arguments.*` form, so every Swift→JS call threw. `8d65bd4`.
3. **Frontmatter notes rendered raw** — the `visit()` skip guard in `blocks.js` was `node.from < ctx.fmRange[1]`; the walk starts at the Document root (`from` 0), so the root itself was skipped → zero structural decorations for any note with frontmatter. Guard is now `node.to <= ctx.fmRange[1]`. `52a5610`.
4. **Wikilink / checkbox clicks dead** — two stacked root causes, both at the bundle level: (a) `WidgetType.eventHandlers()` is a **phantom API** — the pinned @codemirror/view 6.43.14 has no such method (verified in the published dist, the d.ts, and the CHANGELOG), so the widgets' overrides were never called; (b) `WidgetType.ignoreEvent()` defaults to `true` and the view's `eventBelongsToEditor` drops every bubbling event from a widget **before** any `domEventHandlers` runs. Fix: interactive widgets carry their payload as DOM data attributes (`data-target` on `WikilinkWidget`, `data-marker-from` on `CheckboxWidget` — fresh at (re)draw, included in `eq()`), override `ignoreEvent() { return false; }`, and the state-carried view-level `domEventHandlers` decides the click. `2bdaf2f`.

**Final gates at tip `2bdaf2f`** — `swift test` 130/130; `cd Editor && node --test test/` 169/169 (up from 136: frontmatter + widget-click coverage, incl. the `Number("")` trap and a falsy-zero marker-position case); bundle byte-reproducible under `npm ci && npm run build` (CI editor gate green); CI run `38969495221` SUCCESS. The `[DEBUG-editor]` instrumentation was fully stripped — no debug code in any commit.

**Merged** — PR #7 (https://github.com/gianniacquisto/tandem/pull/7, head `2bdaf2f`) merged to `main` as `4c18285` on 2026-10-17, after the human pass was green. The bikeshed feature is done.
