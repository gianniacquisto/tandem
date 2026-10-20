# A todo may link to a bikeshed note

The todo store ([ADR 0001](0001-store-protocol.md)) and the bikeshed ([ADR 0004](0004-vault-protocol.md)) are two surfaces of one app that never crossed: a todo is a title and a done state, a note is a file, and the bikeshed spec lists **"note↔todo cross-references"** explicitly out of scope. We decided to reverse that line, narrowly: a todo may carry an optional `link` — a **vault-root-relative path** to a note, like `projects/App.md`, or `null`. The row shows a quiet paperclip that opens the note in the Bikeshed tab; a context menu sets, changes, and clears it. `link` is **additive** to schema v1, so the envelope stays `version: 1`. It is **GUI-only**: there is no CLI verb, exactly like `update` and `move`.

## Why the link is a path, not a name

- **The vault's identity is the path.** [ADR 0004](0004-vault-protocol.md) fixes it: the filename is the note's identity, and a rename *is* the path change. A stored path is the note's real key, resolvable without an index and legible by hand in the store file.
- **A wikilink target would re-run resolution on every read.** A bare `[[App]]` obeys the wikilink exactly-one rule and can therefore be ambiguous — broken, or worse, silently pointing at a different note once a duplicate appears. The todo would then name something that no longer resolves, which is not a link.
- **A note id would need a metadata store**, which the vault deliberately does not have. Paths are what the tree, the rename transaction, and the editor bridge already speak.

## Why v1 stays v1

- **`link` is additive, and [ADR 0006](0006-stored-order-and-sections.md) made that the rule**: bump the version only when an older reader would **misread** the file, never for a field it can ignore. An installed reader that predates `link` decodes every todo and drops the unknown key; nothing it prints or writes is wrong. Bumping would instead make a v1-strict reader call the file *corrupt* — the exact failure that cost a real store on 2026-10-17.

## Why it is GUI-only

- **The agent surface is `list`/`add`/`complete` ([ADR 0009](0009-cli-agent-face.md)).** Linking a todo to a note is a human act of filing, and an agent that wants the relationship has both the file and the todo on disk. A verb would put a third, redundant way to write the store into the skill for a field no agent needs to set.
- **`link` is still read, not hidden.** The CLI's JSON already carries it (the store encodes the whole `Todo`), so the skill documents `link` on every todo object and says plainly that no verb writes it.

## Rejected

- **A wikilink target (`[[App]]`) in the field.** Ambiguous by construction; the exactly-one rule can break it, and a broken link that still reads as a name is worse than a path the app can test.
- **A note id plus a metadata sidecar.** Reintroduces a metadata directory the vault exists without, for an identity the path already is.
- **Many links per todo.** A todo points at *its* note — one. A set invites a list UI and an ordering, for a relationship that is one-or-none.
- **A note→todo field** (or recomputed backlinks in the Backlinks panel). That is the full cross-reference graph the spec cut, and it makes note content carry todo state. Reversed only in the todo→note direction.
- **Bumping the envelope to 2.** See above: additive fields keep the version.
- **A CLI `todo link` verb.** See above: the agent surface stays three verbs.

## Consequences

- `Todo` gains `link: String?` (Custom Codable, mirroring `section`); `Store.setLink(_:to:)` is the GUI-only operation beside `update`. The store's encoding is untouched — the envelope still writes `version: 1`.
- A renamed or deleted note leaves the stored path dangling: rename rewriting ([ADR 0005](0005-wikilink-rename-rewriting.md)) covers wikilinks, not todo links, and opening a missing path surfaces the core's error. The link is a pointer taken at the time, not a live reference — repairing it is choosing the note again.
- `skills/tandem/SKILL.md` documents `link` in the `list`/`add`/`complete` result shape (it may be `null`); no CLI code changes.
- The out-of-scope line **"note↔todo cross-references"** in the bikeshed spec is reversed in the todo→note direction only; the note→todo direction stays out of scope.

> **Planning notes.** The tracker documents cited above were retired on 2026-10-21 — the bikeshed is canonical for that prose. They remain in this repo's git history; their ground is in the bikeshed's project notes.
