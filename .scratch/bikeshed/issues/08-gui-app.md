# 08 - GUI: Bikeshed tab, tree sidebar, editor host, watcher, backlinks dock

Type: implementation
Status: ready
Blocked by: 06 (needs the `Bikeshed` core API), 07 (hosts the committed editor bundle)

## What to build

The app face for the bikeshed: the Main window's second tab (Todos | Bikeshed), the tree sidebar with search, the WKWebView editor host with the Swift side of the bridge, the recursive FSEvents watcher, the backlinks dock, and the app-side vault protocol behaviour (autosave, conflict banner, Trash deletion, new note / rename / move). The menu bar item stays todos-only.

Scope = the spec's sections: "UI shell — per ticket 02's verdict and the prototype", "Watcher", the app-side of "Vault protocol" (autosave, external changes/conflict, deletion), "Search and backlinks" (UI side), the window-sizing decision, and "Editor — … Bridge" (the Swift side). The spec is the only normative input; where the real core API (ticket 06) or bundle (ticket 07) differs from the spec in signature detail, the in-tree code wins for signatures and the spec wins for behaviour.

Context (pointers):
- Spec: `.scratch/bikeshed/spec.md`
- Bridge contract (binding for this ticket AND ticket 07): `/private/var/folders/4x/kz_77l9s0lddszts3p0k6b940000gn/T/opencode/bikeshed/notes/bridge-contract.md`
- In-tree after 06/07 merge: `Sources/TandemCore/Bikeshed*.swift` (the real core API), the committed editor bundle under the GUI target's resources
- Conventions to mirror: `Sources/Tandem/` (Model, MainView, MenuBar, PopoverFace, StoreWatcher, LigneTheme, LigneControls, SingleInstance)
- Exploration notes: `/private/var/folders/4x/kz_77l9s0lddszts3p0k6b940000gn/T/opencode/bikeshed/notes/gui-structure.md`, `.../ui-prototype.md` (the visual reference; branch `prototype/bikeshed-ui` available for deeper reference)

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
- [ ] `swift build -c release` clean; `swift test` green; the app launches, the tab switches without resizing, and the Bikeshed face works end to end against a real (temp-seeded) vault — record the smoke results in Resolution for the human's manual pass per the spec

## Resolution

(to be filled at merge)
