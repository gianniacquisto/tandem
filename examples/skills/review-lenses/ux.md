# UX lens

The checklist for a diff that changes what the human does: what they can reach, what a wrong click costs, and how they get back. It reads the React source and any screenshot the PR attaches, and judges no screen it cannot see.

## Read

- The keyboard path of everything the diff adds or moves: `app/renderer/src/faces/`, every key handler in `app/renderer/src/keys.ts`, and whether the same action keeps a pointer route.
- Every destructive act: what confirmation it has, and whether `rust/core/src/domain/undo.rs` covers it. A delete, an archive, a lane rename and a store clear each need one of the two.
- Empty states: a lane with no tickets, a search with no match, a bikeshed with no notes.
- Error states: what the human sees when a write fails, `rust/core/src/domain/error.rs` and the wire's `ErrorView` being where the message is made.
- The round-trips: how many clicks or keystrokes a routine act takes after the change, against before it.

## Blocks on

- A destructive act with no undo and no confirmation: a delete outside `rust/core/src/domain/undo.rs`'s coverage and without a dialog on the view that fires it.
- An action the change moves off a keyboard path it had, with nothing put in its place: a `⌘K` route or a `.keyboardShortcut` removed where the diff's own text claims no alternative.
- A state the human can reach with no way out: an error banner with no dismiss, an empty state with no route to the thing it names.
- A failure that is swallowed, so the action looks done: a write's error dropped instead of reaching the wire (`rust/core/src/domain/error.rs`).
- A screen the PR's screenshot shows broken for the human's path (a disabled control that is the only route, a label that names the wrong action), judged as the diff's own change.

## Never blocks on

- A shortcut the reviewer would have chosen differently, when the action keeps a visible route.
- A confirmation dialog where an undo would have done, when one of the two is there.
- Wording the reviewer likes better.
