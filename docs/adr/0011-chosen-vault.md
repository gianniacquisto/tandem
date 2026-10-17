# The notes vault is a chosen folder: persisted in settings, switched live

[ADR 0004](0004-vault-protocol.md) fixed the vault as `<home>/bikeshed/` — one folder, right beside the todo store — and `.scratch/bikeshed/spec.md` cut *"Multiple vaults — one fixed folder"* out of scope on purpose. A human already keeps notes in Obsidian vaults, iCloud folders, and Dropbox; Tandem only being useful if the notes live in *its* folder is a real adoption cost. We decided: **the vault is now a folder the human chooses from the app**, persisted so it survives restarts, and **applied live** — no relaunch. The default is unchanged: with no choice made, the vault is still `<home>/bikeshed`. The choice lives in a tiny `<home>/settings.json` (`{"vaultPath": "<absolute path>"}`), sibling of the store and the lockfiles, never inside the vault itself.

## Why

- **The folder is the interface, so the interface can be anywhere.** ADR 0004 made the bikeshed "a plain folder of notes"; nothing about the protocol depends on the folder being under `~/.tandem`. Pointing Tandem at an existing Obsidian vault is then just a path — the same files, the same watcher, the same wikilinks.
- **Settings are not the store.** The todo store is a versioned, schema'd domain file with recovery semantics (ADR 0001). A chosen path is not domain data. It gets its own small, schema-tolerant file: one key today, one more key tomorrow, never a store-version bump. A missing, unreadable, or malformed `settings.json` reads as "no choice" (`nil`) and falls back to the default — a broken settings file must never take the app down.
- **The lock must not land in the user's folder.** ADR 0004 keeps the sidecar lock *outside* the vault (`~/.tandem/bikeshed.lock`) so the vault holds note content only. A chosen vault (say `~/Documents/Notes`) must not get a `bikeshed.lock` dropped beside it. So the lock stays in Tandem's home for every vault — `<home>/bikeshed.lock` — which is exactly what the default already resolves to. Behaviour with no choice is byte-identical.
- **Live switch, not a relaunch.** The GUI rebuilds the vault value and its recursive `VaultWatcher` in place: flush the open note first (a switch never discards unsaved work), persist the choice, build a fresh `Bikeshed` on the chosen folder, reset the tree and editor, and re-send the vault index. No restart, no window teardown.
- **A file, not `UserDefaults`.** Defaults are keyed by bundle id and shared across builds, so a from-source run could redirect the shipped app's vault — exactly the cross-contamination the `TandemHome` split exists to prevent (ADR 0001). A file under the process's own home is per-home for free, and it is inspectable with `cat`.
- **Agents are unaffected.** The vault is still a plain folder of `.md` files read and written by ordinary file tools ([ADR 0009](0009-cli-agent-face.md)'s "the filesystem is the notes interface"). Agents reach whichever folder is chosen by its path; nothing about their contract — or `tandem-cli note rename` — changes. A memory/session skill that points at the default needs the chosen path only when the human moved the vault.
- **The default stays the default.** With no `vaultPath` key, the vault is `<home>/bikeshed` exactly as before, so existing users, agents, and every core test are unaffected. A choice is opt-in and reversible — choose the default folder back to return to it.

## Why one vault, not many

This is deliberately the small half of "multiple vaults": there is still **one** vault at a time — a single tree, one watcher, one open note — and the human only chooses *which* folder it is. A vault list, per-project switching, or recent vaults would each add selection state the tree, search, and backlinks do not have today, and is a separate change. The reverse is also true: because only one folder is watched, the previous folder simply stops being watched on a switch (no copy, no delete, nothing migrated).

## Rejected

- **Keep one fixed folder.** The adoption cost is real, and there is no protocol reason for it — the vault is already location-independent.
- **Store the choice in the todo store envelope.** Conflates two schemas: a path change would touch the notes' file format, and a store-version bump would be paid for a setting (ADR 0006's "additive" discipline says no).
- **Put the lock beside the chosen vault.** Drops a Tandem artifact into a folder the human owns and may sync; the ADR 0001 sidecar pattern is deliberately about *Tandem's* home.
- **Require a restart (read the choice at launch only).** A restart to change a folder is the ceremony this change exists to remove, and the watcher rebuild is small.
- **Move or copy the notes on a switch.** Switching selects a folder; it never touches file contents. A human pointing at an existing Obsidian vault wants their notes where they are.
- **A sandbox / security-scoped bookmark.** Out of scope: the app is non-sandboxed and reads/writes paths directly (spec, "Out of Scope").

## Consequences

- `Sources/TandemCore/TandemSettings.swift` owns the record: `chosenVault()` / `setChosenVault(_:)`, an explicit-home seam for tests, atomic same-directory temp + `rename(2)` writes.
- `Bikeshed.init()` resolves `TandemSettings.chosenVault() ?? <home>/bikeshed`; a new `init(directory:lockURL:)` takes the home-based lock. `init(directory:)` (the tests' explicit-dir initialiser) is unchanged and keeps its sibling lock — existing tests and their temp-dir isolation are untouched.
- The GUI gains "Choose Bikeshed Vault…" (⌘⇧V) in the File menu, an `NSOpenPanel` folder picker, and `BikeshedModel.switchVault(to:)`.
- A switch closes the open note (after flushing it) and clears expanded folders — the tree belongs to the vault it came from. The chosen path is what the empty-vault state shows, so an empty chosen folder still reads as intentional.
- `.scratch/bikeshed/spec.md`'s *"Multiple vaults — one fixed folder"* out-of-scope line is reversed: one vault at a time, but a folder the human chooses.
- `settings.json` is per-home, so a dev build (`~/.tandem-dev`) and the shipped app (`~/.tandem`) keep independent choices — the ADR 0001 dev/prod split holds.

Amends [ADR 0004](0004-vault-protocol.md)'s fixed-vault clause: the lock and the protocol are unchanged, the folder is no longer fixed. Reverses the *"Multiple vaults — one fixed folder"* scope cut in `.scratch/bikeshed/spec.md`.
