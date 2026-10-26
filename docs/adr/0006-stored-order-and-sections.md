# Order and sections: the stored order is the order, and sections group it

> **Amended 2026-10-26 by [ADR 0031](0031-one-database-the-store.md):** the envelope is gone, so the order lives in columns — `tickets.position` is the list order and `lanes.position` the lane order — and `schema_migrations` replaced the envelope's `version` as the migration seam. The decision stands: the stored order is the order, never derived from `createdAt`. The amendment's text is ADR 0031's.

[ADR 0001](0001-store-protocol.md) fixed the store envelope as `{"version": 1, "todos": [...]}` in **creation order**, "always" — the reason a captured list matched the order things were captured. This ADR supersedes that clause. We decided: the position of a todo in the `todos` array **is** its position in the list; `add` puts a new todo **on top** by default; the order is never recomputed from `createdAt`. Sections group the list: the envelope carries an ordered `sections: [String]`, each todo an optional `section` name, and `null` is the default (ungrouped) area, always first. The array is kept **grouped by section** — default first, then each declared section in order — so a reader walks it straight through and the file stays legible by hand. `sections` and `section` are **additive**, so **the envelope version does not move**: it stays `1`. Everything else from ADR 0001 (sidecar lock, atomic rename, recovery, lenient read) is unchanged.

## Why these boundaries

- **Creation order, always (ADR 0001)** — a captured list matched the order things were captured, and agents got a stable order for free. Superseded because the list is one a human *arranges*: new work should surface on top, and a drag has to mean something. Both require a stored order.
- **Derive the display order from `createdAt`** — no schema change, but a manual reorder has nowhere to live: the next sort undoes it. The stored order is the only record of intent, so the array has to be it.
- **Sections as a list of `{name, todos}` buckets** — would nest the data and break the flat `todos` array the MCP surface returns and the "one legible file" property. A name on a flat todo keeps both.
- **Section ids instead of names** — ids survive a rename for free, but make the file opaque to a human editor and add a lookup to every read. A named section renames in one atomic write (the list plus every member); the file stays readable.
- **Deleting a section deletes its todos** — rejected: deleting a container is not a request to destroy its contents. Members move to the default area, and `deleteSection` returns how many moved.
- **Schema `version: 2`** — rejected, and this is the one that cost real data. The version is a *compatibility signal*: bump it only when an older reader would **misread** the file. Sections are additive — an older v1 build ignores the unknown `sections`/`section` keys and reads the file correctly — so a bump tells it nothing it can act on, while a reader that insists on `version == 1` decides the file is *corrupt*. That is what happened on 2026-10-17: an installed v1-era `tandem-mcp` met a version-2 file, backed it up as `todos.json.corrupt-<ts>` and started empty. The stored order rides under version 1 for the same reason: an older reader returns the array in file order, which *is* the stored order. **Additive fields keep the version; only a breaking shape earns a bump.** Reading tolerates `2`, so files written during that window still load.

## Consequences

- The order reaches every reader, agents included: MCP `list` returns the **stored** order. Its description and `add`'s say so.
- A hand-edited file self-heals: any write regroups the array (default, then declared sections, then any dangling name). Interleaving is tolerated on read, normalized on write.
- Section CRUD (`addSection`/`renameSection`/`deleteSection`) and both ordering ops (`move`, `moveSection`) are GUI-only — no MCP tools. The agent surface stays `list`/`add`/`complete`; an agent `add` lands on top of the default area.
- `move(id, toSection:at:)` is the one ordering op for todos — reorder-in-place and move-between are the same call.
- `moveSection(name, to:)` reorders the `sections` list; the groups travel whole, so the order of todos *within* a section never changes. The default area is not a section and cannot be moved — it is always first.
- A file written before sections existed reads unchanged (todos simply carry no `section`), and an older build reading a current file loses no todos — at worst it drops the section names when *it* next writes the file.

## Amendment, 2026-10-20: lane lifecycle reaches the agent face

The first clause of the consequence above — section CRUD is GUI-only, no agent verbs — is reversed for the three lifecycle operations. `tandem-cli lane add --name`, `lane rename --from --to`, and `lane delete --name` call the same `addLane` / `renameLane` / `deleteLane` the app's tree calls, so trimming, the empty-name and duplicate rejections, the unknown-lane rejection, and "delete moves members to the default area, never deletes them" are one implementation, not two. `moveLane` (reordering the rows) stays GUI-only: the ask was creation, not arrangement.

Each verb returns the lane list after the write (`{"lanes": [...]}`; `delete` adds `"moved": <n>`), so an agent learns the resulting names without a second, unlocked read — which would also have dropped the corrupt-store recovery note the write carried.

Lanes cut by project made the gap concrete: creating the first project lane took a hand edit of the store under the lock, because the app was the only writer of the lane list. A lane is still **not** a permission boundary: any writer may move any ticket between lanes, and requiring validation to leave a lane would need an identity to validate against, which `assignee` does not yet have ([ADR 0017](0017-the-ticket-model.md), personas). That rule is deliberately out of scope here.
