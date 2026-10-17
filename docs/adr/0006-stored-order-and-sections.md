# Order and sections: the stored order is the order, and sections group it

[ADR 0001](0001-store-protocol.md) fixed the store envelope as `{"version": 1, "todos": [...]}` in **creation order**, "always" — the reason a captured list matched the order things were captured. This ADR supersedes that clause. We decided: the position of a todo in the `todos` array **is** its position in the list; `add` puts a new todo **on top** by default; the order is never recomputed from `createdAt`. Sections group the list: the envelope carries an ordered `sections: [String]`, each todo an optional `section` name, and `null` is the default (ungrouped) area, always first. The array is kept **grouped by section** — default first, then each declared section in order — so a reader walks it straight through and the file stays legible by hand. The file is schema **v2**; a v1 file still reads and is upgraded on the next write. Everything else from ADR 0001 (sidecar lock, atomic rename, recovery, lenient read) is unchanged.

## Why these boundaries

- **Creation order, always (ADR 0001)** — a captured list matched the order things were captured, and agents got a stable order for free. Superseded because the list is one a human *arranges*: new work should surface on top, and a drag has to mean something. Both require a stored order.
- **Derive the display order from `createdAt`** — no schema change, but a manual reorder has nowhere to live: the next sort undoes it. The stored order is the only record of intent, so the array has to be it.
- **Sections as a list of `{name, todos}` buckets** — would nest the data and break the flat `todos` array the MCP surface returns and the "one legible file" property. A name on a flat todo keeps both.
- **Section ids instead of names** — ids survive a rename for free, but make the file opaque to a human editor and add a lookup to every read. A named section renames in one atomic write (the list plus every member); the file stays readable.
- **Deleting a section deletes its todos** — rejected: deleting a container is not a request to destroy its contents. Members move to the default area, and `deleteSection` returns how many moved.

## Consequences

- The order reaches every reader, agents included: MCP `list` returns the **stored** order. Its description and `add`'s say so.
- A hand-edited file self-heals: any write regroups the array (default, then declared sections, then any dangling name). Interleaving is tolerated on read, normalized on write.
- Section CRUD (`addSection`/`renameSection`/`deleteSection`) and `move` are GUI-only — no MCP tools. The agent surface stays `list`/`add`/`complete`; an agent `add` lands on top of the default area.
- `move(id, toSection:at:)` is the one ordering op — reorder-in-place and move-between are the same call.
