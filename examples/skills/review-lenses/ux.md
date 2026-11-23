# UX lens

The checklist for a diff that changes what the human does: what they can reach, what a wrong click costs, and how they get back. It reads the templates, the client script and any screenshot the PR attaches, and judges no screen it cannot see.

## Read

- The keyboard path of everything the diff adds or moves: the templates, every key handler in the client script, and whether the same action keeps a pointer route.
- Every destructive act: what confirmation it has. A delete, an archive, a rename and any act the product cannot undo each need a confirmation on the route that fires them.
- Empty states: a list with no items, a search with no match.
- Error states: what the human sees when a write fails, and the code where the repo makes that message.
- The round-trips: how many clicks or keystrokes a routine act takes after the change, against before it.

## Blocks on

- A destructive act with no confirmation and no route back: a delete or a rename fired straight from a control, with no dialog and no way to reverse the write.
- An action the change moves off a keyboard path it had, with nothing put in its place, where the diff's own text claims no alternative.
- A state the human can reach with no way out: an error banner with no dismiss, an empty state with no route to the thing it names.
- A failure that is swallowed, so the action looks done: a write's error dropped before it reaches the human.
- A screen the PR's screenshot shows broken for the human's path (a disabled control that is the only route, a label that names the wrong action), judged as the diff's own change.

## Never blocks on

- A shortcut the reviewer would have chosen differently, when the action keeps a visible route.
- A confirmation dialog where an undo would have done, when the route has one of the two.
- Wording the reviewer likes better.
