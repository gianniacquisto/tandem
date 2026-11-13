# UX lens

The checklist for a diff that changes what the human does: what they can reach, what a wrong click costs, and how they get back. It reads the Askama templates, `rust/core/src/serve/static/tandem.js` and any screenshot the PR attaches, and judges no screen it cannot see.

## Read

- The keyboard path of everything the diff adds or moves: `rust/core/templates/`, every key handler in `rust/core/src/serve/static/tandem.js`, and whether the same action keeps a pointer route.
- Every destructive act: what confirmation it has. Completion has no undo (ADR 0045); a delete, an archive and a lane rename each need a confirmation on the route that fires them.
- Empty states: a lane with no tickets, a search with no match, a bikeshed with no notes.
- Error states: what the human sees when a write fails, with `rust/core/src/domain/error.rs` and the wire's `ErrorView` being where the message is made.
- The round-trips: how many clicks or keystrokes a routine act takes after the change, against before it.

## Blocks on

- A destructive act with no confirmation and no route back: a delete or a lane rename fired straight from a control, with no dialog and no way to reverse the write.
- An action the change moves off a keyboard path it had, with nothing put in its place, where the diff's own text claims no alternative.
- A state the human can reach with no way out: an error banner with no dismiss, an empty state with no route to the thing it names.
- A failure that is swallowed, so the action looks done: a write's error dropped instead of reaching the wire (`rust/core/src/domain/error.rs`).
- A screen the PR's screenshot shows broken for the human's path (a disabled control that is the only route, a label that names the wrong action), judged as the diff's own change.

## Never blocks on

- A shortcut the reviewer would have chosen differently, when the action keeps a visible route.
- A confirmation dialog where an undo would have done, when the route has one of the two.
- Wording the reviewer likes better.
