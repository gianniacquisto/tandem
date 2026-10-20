# The hidden layer is revealable in the tree, and stays out of the graph

[ADR 0016](0016-note-capture.md) hid the capture layer — `projects/<project>/.raw/`, with the consolidation archive at `projects/<project>/.raw/.done/` — so it is out of the curated wikilink graph **by construction**: `Bikeshed.isHidden` prunes the whole subtree from the tree, from `notePaths`, and therefore from `search`, from backlinks, and from wikilink resolution.

That is right for reading. It left the human with no route to the queue from the app: opening Finder, or running `note stats`, was the only way to see what the agents had captured.

We decided: **a persisted setting reveals the hidden layer in the tree, off by default.** The flag decides what the tree *draws*; it never decides what the curated graph *is*.

## Decision

- **One key, absent means off.** `settings.json` gains `"showHiddenFiles"` (ADR 0011's settings record — one more key, not a schema change). Absent is the default: the curated layer stays the reading surface, and the hidden layer is something the human asks to see.
- **The flag lives on the core bikeshed.** `Bikeshed.showsHiddenFiles` is read from the persisted setting at init and set by the GUI; the tree walk takes it as `includeHidden`. Hidden entries — folders and `.md` notes — are drawn when it is on, and pruned when it is off. Hidden *non-markdown* files stay attachments (invisible), as they always were.
- **The linkable set never changes.** `Bikeshed.notePaths()` always prunes the hidden layer, in both states, so search, backlinks, wikilink resolution, and the editor's note index are exactly what they were. A revealed capture is drawn, openable, and writable, and it is still not a link target and never a search hit.
- **A revealed capture is a normal place to work.** It opens in the editor and autosaves like any note; deleting it from the tree is the same act as deleting the file (the Trash move first, the core's permanent delete as the fallback). Creating inside a revealed hidden folder is allowed: the naming rule that refuses hidden components exists because a hidden note would be "created yet never visible", and that rationale does not hold once it is visible.
- **The control is in the app's settings surface** — a `Settings…` sheet (⌘,), one checkbox, "Show hidden files", stating what the reveal does and what it does not (search, backlinks, and wikilinks are untouched).

## Why

- **The hidden layer is a working surface, not only a machine surface.** A human reads a capture before the consolidation pass folds it in, fixes a typo in one, or clears one that was noise. Those are acts the app should support; today the human has to leave the app and use the filesystem.
- **The graph rule is untouched and code-enforced.** The flag never reaches `notePaths()`, so the invariant ADR 0016 bought — a capture cannot pollute the curated graph even if an agent forgets — is unchanged. The reveal is a *view* change, and the code says so in one place (`includeHidden`).
- **Off by default is the point.** The curated notes are the layer meant to be read; a tree full of raw captures would be noise. The human asks for the queue when they want the queue.
- **One setting, one key, one place.** The flag is persisted like the other settings (`TandemSettings`), read by the core, applied live: turning it on redraws the tree without a restart, exactly like choosing the bikeshed folder.

## Rejected

- **Showing hidden files by default.** The curated layer must stay the default reading surface.
- **Making the captures linkable while revealed.** The reveal is for reading and tidying the queue. Link targets are the curated graph's business, and a capture that suddenly became a link target would change what a wikilink means — the thing ADR 0016 deliberately fenced off.
- **A separate "hidden" view or filter instead of drawing them inline.** A second tree is a second surface to learn, and the hidden layer's shape is already a tree; the human asked to *see* it in the one they already have.
- **A CLI verb to toggle the flag.** The settings file is the shared record, and the human is the one who reads the queue — the app's settings surface is where the choice belongs. (An agent that wants the raw layer already has `note stats` and the filesystem.)
- **Revealing only `.raw/` and `.done/` by name.** A special case for two folder names is a rule agents and humans must remember; "hidden entries are visible" is one rule, and it is the one the setting is named after.

## Consequences

- **New core surface**: `TandemSettings.showHiddenFiles()` / `setShowHiddenFiles(_:)` (with the explicit-home test seam), `Bikeshed.showsHiddenFiles`, and `Bikeshed.isHiddenPath(_:)` — the path-level form of the hidden rule, so a caller holding a tree can ask the same question the walk asks.
- **The GUI**: `BikeshedModel.showsHiddenFiles` and `setShowsHiddenFiles(_:)` (persist, apply, redraw); the tree draws hidden rows dimmed with a tooltip saying they are outside search, backlinks, and wikilinks; the editor's note index is filtered so a revealed capture is never a link target.
- **The reveal is generic, not memory-specific.** Any dot-folder in the bikeshed is drawn when the flag is on — a bikeshed that is also a git repository shows `.git` too. That is the price of one rule instead of a special case; the flag is off by default, and the human turns it on when they want to inspect the queue.
- **Tested**: `BikeshedHiddenLayerTests` covers the tree in both states, the unchanged linkable set, search, backlinks, and wikilink resolution in both states, read-write on a revealed capture, delete-as-file-deletion, and the create rule following the flag. `TandemSettingsTests` covers the persisted key and its coexistence with the other settings.
