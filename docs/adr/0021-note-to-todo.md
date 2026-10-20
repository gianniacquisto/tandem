# A note can become a ticket — the reverse of the todo→note link

[ADR 0010](0010-todo-note-links.md) made a todo point at a bikeshed note, and closed with the line that "the note→todo direction stays out of scope". This reverses that line, narrowly: **a piece of a note can be filed as a ticket**, and the ticket points back at the note it came from.

## Why

The todo→note link made the ticket the anchor: a ticket can say which note it is about. The other direction had no way in. A paragraph, a decision, or a checkbox line that lives in a note cannot become a ticket without retyping it, so capture from inside the editor is a copy-paste — and copy-paste loses the link back to the note, which is the half of the relationship that makes the note worth keeping. The editor is where the thought is; the list is where the work is; the human should not have to travel between them by hand.

## Decision

- **One core operation, `Store.addFromNote(selection, notePath:)`** (GUI-only, like `setLink` and `update`). The editor hands over *text*; the core decides what a ticket made of that text looks like. One operation behind both triggers, so the menu item and the shortcut cannot drift.
- **The title is the text's first non-blank line**, trimmed, **with a leading task-list marker removed** (`- [ ] `, `* [x] `, `1. [ ] `). A checkbox line's *text* is the ticket's title; the syntax is the note's business, not the ticket's. Blank leading lines are skipped, so a selection that starts with a newline still titles the ticket after its first real line.
- **The description is the whole selection**, trimmed — markdown, as written.
- **The link is the note's bikeshed-root-relative path** (`projects/App.md`), the same shape `setLink` stores (ADR 0010). A path that is not bikeshed-relative is rejected, so the link is always a path the note tree can open.
- **The ticket lands on top of the default lane, in `backlog`** — where the quick-add files it. An empty selection is rejected: there is no title to file, and the message says so rather than filing an empty ticket.
- **The note is not modified.** Nothing is written into the note, no marker is inserted, no state is kept in the note. The operation writes the store and nothing else.
- **Two triggers, one operation.** The editor's title bar carries a "Send to todos" item; the editor's own ⌘⇧T shortcut does the same thing from the keyboard. The shortcut lives in the page's keymap, because that is where the selection is; the menu item asks the page for the text it would file, and the page answers over the same message the shortcut posts. Both file the same ticket through the same core call.
- **The agent surface is unchanged.** There is no CLI verb: the ask is the human's gesture in the editor, and an agent that wants a ticket from a note writes the note. A verb can follow if the operation proves useful from that side too.

## Decided against

- **A CLI `note to-todo` verb** — the GUI action is the ask; a third way for an agent to write the store is not, and the note is already on disk for the agent that wants it.
- **Writing a marker into the note** (a `> - [ ]` line, a `todo:` directive) — the note would then carry todo state, which is the cross-reference graph ADR 0010 cut: the note's text would mean different things depending on what the list holds.
- **The shortcut in the app's menu** (a Swift key equivalent) — the webview owns the selection, and a page keymap fires where the selection is; a Swift-side ⌘⇧T would risk firing the same action twice.
- **A token in the note that resolves to a ticket** — same objection as the marker: a note that names tickets is a graph, and the graph is out.

## Consequences

- The store's shape is unchanged: `link` and `description` are existing additive fields, so the envelope stays `version: 1` (ADR 0006) and no migration is needed.
- `Store.addFromNote` is a new core operation, tested headlessly (title trim, empty selection rejected, bikeshed-relative link, the note's bytes untouched); the GUI has no test target, so the bridge and the two triggers are proven by hand.
- The editor bundle is regenerated: the keymap and the `requestTodoCandidate()` page global travel with it. A page global added on one side and forgotten on the other fails silently (the bundle is generated, and the Swift call no-ops) — the same trap as the `setNoteIndex` rename.
- The two directions now meet: a ticket filed from a note carries the note's path, and the row's paperclip opens that note (ADR 0010's reader). A note can be the origin of a ticket without the ticket leaving the note.
