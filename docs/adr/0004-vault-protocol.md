# Vault protocol: per-note files, one-sided sidecar lock, journal-free rename

The bikeshed (`~/.tandem/bikeshed/`) is a plain folder of `.md` notes (filename = note identity, nested folders allowed, non-`.md` files are attachments that are invisible in the tree) written by the app and by agents/editors that share no API with it. We decided: every app write takes an exclusive `flock` on the sidecar lockfile `~/.tandem/bikeshed.lock` (outside the vault, never renamed — the ADR 0001 pattern) held across the whole write, multi-file operations included; every write is a same-directory temp file + fsync + `rename`, and renames are plain `rename(2)`; rename-with-link-rewrite runs as one transaction under the lock, in the order ADR 0004 settles (rewrite the referencing notes first, then rename the note), with **no journal** — a crash mid-sequence yields at most broken links, never torn files or lost data.

## Why the lock is one-sided

Agents and external editors write files directly and take no lock (there is no API for them to in v1). The lock therefore provides no mutual exclusion against the majority of writers; it exists to (a) serialize the app's own writes — especially multi-file rename-with-rewrite — against other app operations, and (b) give a documented hook in case MCP note tools ever return. Protection against external writers is atomic reads (app writes are never torn), the FSEvents watcher, and the editor's conflict policy: a clean buffer adopts an external change silently; a dirty buffer is never clobbered — a banner offers *overwrite with mine / take theirs*, and a pending autosave is withheld until the human chooses. Per-file locks or a writer registry to police out-of-process writers were rejected: they would add ceremony for writers that don't exist.

## Consequences

- The lockfile lives outside the vault, so the vault folder contains note content only; the tree ignores hidden files and non-`.md` files.
- Deletion is deliberately asymmetric: UI delete moves the file or folder to the macOS Trash (recoverable); agent deletion is a plain `rm` (permanent). No tombstones — a deleted note is simply a broken wikilink.
- Naming rules constrain what the app *creates* (no `/`, no `#`, no `|` — the latter two are the wikilink grammar's separators, ADR 0004); pre-existing violators are still shown and edited.
- A missing vault folder is an empty vault: the app creates it lazily and seeds no welcome note.
