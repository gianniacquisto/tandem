# Bikeshed

The notes vault in Tandem: a plain folder of `.md` notes at `~/.tandem/bikeshed/`, watched live by the app, reached by agents as on-disk files. Tracker: `tandem/.scratch/bikeshed/` — a wayfinder map; this note is the memory of the work, the map is the tracker, and the two stay separate.

## Where left off

The map is fully resolved (2026-10-15) and the spec is written at `tandem/.scratch/bikeshed/spec.md`. Next: implement in the spec's seam order — the `Bikeshed` core type in TandemCore, then the editor bundle, then the GUI.

## Decisions

- **Inline live preview is the primary path** — WKWebView + CodeMirror 6, Obsidian's stack; the sanctioned split view is the fallback. Native-only TextKit 2 has no public hook to hide syntax markers.
- **Wikilinks resolve live, case-insensitively, against the file set.** A bare target needs a unique basename; a duplicated basename is a *visible broken link*, never a silent tie-break — deterministic, fixable in one edit.
- **App-performed renames rewrite every wikilink that resolves to the renamed note; agent renames rewrite nothing.** The app cannot tell a deliberate rename from on-disk churn, so only app renames trigger the rewrite; a crash mid-sequence leaves broken links a retry self-heals.
- **The lock is one-sided** — app writes hold a sidecar `flock` on `~/.tandem/bikeshed.lock`; agents write plain files, last write wins. The lock serializes the app's own multi-file renames and is a documented hook should MCP note tools ever return.

## Gotchas

- The existing `StoreWatcher` covers one directory level; a nested vault needs a recursive FSEvents watcher.
- A broken wikilink's click *creates the note at its path* — a missing `[[projects/Beta]]` is an affordance, not an error to fix.
- A note whose name contains `#` or `|` is unnameable by wikilink; keep names free of both.

## Facts

- The CodeMirror 6 bundle is ~600 KB of JS, vendored at `tandem/Sources/Tandem/Resources/Editor/`.
- The UI prototype (the bikeshed surface mock) lives on branch `prototype/bikeshed-ui` — throwaway, never merged ([[Throwaway Branch Workflow]]).
- The test seam: a `Bikeshed` type in TandemCore, sibling of `Store` — `Sendable`, directory-rooted, `init(directory:)` for temp-dir tests; the core is the only unit-tested surface.

## Threads

- Land the editor bundle (CodeMirror assets + bridge) as a bundled resource.
