# Orientation walk — cold start

What a new session does when it picks up work that touches the vault. This is the shape (ticket 01); how much to load and when to stop is the read protocol (ticket 03).

1. **Open `Index.md`.** The one fixed constant every session knows: the entry point at the vault root. It holds pointers and one-line status only — a small read that names every project the agent has memory for, and where the topics live.
2. **Match the task to a project line.** The matching line's wikilink is the project's anchor note. No match → the task is new: check `topics/` for reusable knowledge, and at session end create `projects/<Project>.md` plus an Index line (write protocol, ticket 02).
3. **Read the anchor top to bottom.** `Where left off` = state + next move. `Decisions`, `Gotchas`, `Facts` = reference, consulted as the task needs them, not a required read.
4. **Follow the anchor's links outward.** Each wikilink the anchor names leads to its note. In the app, the note's **Backlinks dock** shows the neighbourhood in one hop: every note that links here.
5. **Check the todos.** Active threads live in the Tandem todos (MCP `list`), not in the vault; the anchor's `Threads` section names which threads belong to the project.

Bounded by construction: one small entry note, one anchor note, at most the notes the anchor names. The vault tree is the human's index; the Index note is the agent's.
